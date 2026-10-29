//! The window: top bar, shape palette, viewport, side panels, shortcuts, undo and files.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{Color32, RichText, Ui};

use crate::camera::Camera;
use crate::gizmo::{Drag, Tool};
use crate::model::{Group, Kind, MAX_GROUPS, MAX_SHAPES, Material, Op, Pick, Scene, Shape};
use crate::sdf::Field;
use crate::view::centre_of;
use crate::{glyphs, i18n, style};

#[cfg(test)]
#[path = "app_preview.rs"]
mod preview;

#[path = "palette.rs"]
mod palette;

const UNDO_LIMIT: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportFormat {
    Stl,
    ThreeMf,
    Obj,
}

#[allow(clippy::large_enum_variant)] // short-lived; boxing would only add noise
enum PendingFileAction {
    New(Scene),
    Open(PathBuf),
    Close,
}

struct QueuedOpen {
    path: PathBuf,
    base_revision: u64,
}

#[derive(Clone, Copy)]
enum PendingFileDecision {
    Save,
    Discard,
    Cancel,
}

pub struct App {
    pub scene: Scene,
    /// The scene as of the last finished edit (what undo returns to).
    committed: Scene,
    undo: Vec<Scene>,
    redo: Vec<Scene>,
    pub cam: Camera,
    pub sel: Option<Pick>,
    /// Temporary extra picks, valid only while `multi_for` still matches the primary `sel`.
    pub multi: Vec<Pick>,
    pub(crate) multi_for: Option<Pick>,
    pub tool: Tool,
    /// What a new shape does: add to the group or carve from it.
    pub new_op: Op,
    pub drag: Option<Drag>,
    pub srgb: bool,
    /// Analysis shading of the viewport (zebra, rings, up / down): a viewer setting, not part of the design.
    pub view_mode: crate::view_mode::ViewMode,
    pub last_view_height: f32,
    /// The scene as of the last `track_scene` and its revision number: a change is found once per
    /// frame by comparing, and everything else (the picture, the field, the undo step) asks the
    /// number instead of comparing whole scenes again.
    tracked: Scene,
    scene_rev: u64,
    /// The revision `committed` is current for, and the one `dedupe_uids` last ran for.
    committed_rev: u64,
    deduped_rev: u64,
    /// Which document the scene shows. A new or opened design (or a control command that
    /// replaces the scene) starts the next one, so work started for the old document can tell
    /// that its result must not be put into the new one.
    document_generation: u64,
    /// The field for picking, made when picking needs it and the scene has changed since.
    field_cache: std::cell::RefCell<Option<(u64, std::rc::Rc<Field>, Scene)>>,
    /// How many times the field was built (tests and the HUD look at it).
    pub(crate) field_builds: std::cell::Cell<u64>,
    pub path: Option<PathBuf>,
    /// The name typed into the title for a design that has no file yet (empty = none).
    doc_title: String,
    /// The title being typed (Some while the name box in the top bar is open).
    title_edit: Option<String>,
    pub dirty: bool,
    recovery: crate::recovery::Store,
    pending_file_action: Option<PendingFileAction>,
    /// Native document parsing and saving run off the UI thread.
    native_job: Option<crate::document_job::Job>,
    queued_open: Option<QueuedOpen>,
    /// A Save-and-continue decision only runs after a complete successful save.
    after_save: Option<PendingFileAction>,
    close_approved: bool,
    status: (String, f64),
    pub renaming: Option<Pick>,
    /// Box selection: which kind is on, and the corner where the drag began.
    pub marquee_from: Option<egui::Pos2>,
    pub marquee_to: egui::Pos2,
    /// How the view is turned and zoomed: the mouse's way or the trackpad's way (or follow what is used).
    pub input_dev: crate::view::InputDev,
    /// What the last scroll looked like (when the choice is Auto).
    pub input_seen: crate::view::Device,
    pub folded: HashSet<usize>,
    /// A row of the scene list being dragged, and where every row is on screen this frame.
    pub layer_drag: Option<Pick>,
    pub row_rects: Vec<(Pick, egui::Rect)>,
    shot: Option<PathBuf>,
    /// The viewport's place on screen (for a picture of it).
    pub view_rect: egui::Rect,
    /// A picture being taken: the overlays are hidden until it arrives.
    pub capture: Option<Capture>,
    /// Commands from `--control` (an AI or a script), run on the window thread.
    pub an: crate::anim_ui::State,
    /// SolidCraft's solid modeller, shown instead of the melt view while it is on.
    pub precise: Option<crate::precise::Precise>,
    pub precise_on: bool,
    pub(crate) precise_services: Option<solidcraft_ui_egui::Services>,
    pub(crate) precise_prefs: Option<String>,
    pub control: Option<std::sync::mpsc::Receiver<crate::control::Request>>,
    /// A picture requested over the control channel: drawn and written on a worker.
    render_job: Option<crate::render_job::Job>,
    /// The shape under the mouse.
    pub hover: Option<Pick>,
    /// Where the camera is gliding to.
    pub fly: Option<Camera>,
    /// A left-drag that began on empty space is turning the view.
    pub orbiting: bool,
    pub show_keys: bool,
    /// Pinned palette entries and custom shortcuts.
    pub prefs: crate::palette_prefs::Prefs,
    /// Language, display, snapping, navigation, autosave and export defaults.
    pub preferences: crate::prefs::Preferences,
    /// Which tab of the right panel is showing.
    pub tab: Tab,
    /// The material list's chosen category (None = the person's saved ones).
    pub mat_cat: Option<crate::materials::Category>,
    /// A material picked from the list goes to the whole group.
    pub mat_group: bool,
    /// Show the materials lightest first.
    pub mat_sort: bool,
    /// The curve tool (nodes, drawing).
    pub ct: crate::curve_tool::State,
    /// The strip shows only the tools that make solids from a sketch.
    pub solid_mode: bool,
    /// The pipe (trace) tool.
    pub pt: crate::pipe_tool::State,
    /// The sketch tool (plane, pen, finished profile) and the outline editing.
    pub sk: crate::sketch_ui::State,
    /// Per-user opt-out, non-modal first-run guide and contextual operation hints.
    pub guide: crate::guide::Guide,
    /// Frame cache, fast mode, quality and other speed-ups (see `perf.rs`).
    pub perf: crate::perf::Perf,
    /// The last copy (text), so the Edit menu can paste without asking the system.
    pub clipboard: Option<String>,
    /// The library card (ready-made models): open or not, which category, the search text.
    pub lib_open: bool,
    pub lib_cat: usize,
    pub lib_query: String,
    /// Font discovery runs on a worker; do not wait for it while opening the window.
    font_job: Option<crate::fonts::FontJob>,
    /// A message from code that has no window context (shown in the status bar next frame).
    pub notice: Option<String>,
    /// The Melt tab's picture of the picked shapes melting (its fingerprint and the image).
    pub(crate) melt_preview: Option<(u64, egui::TextureHandle)>,
    /// The graphics path (GPU picture or the simple CPU one) and its cards.
    pub gpu: crate::gpu_ui::GpuUi,
    /// The window is closing because its shared graphics device can no longer draw.
    /// Keep the latest recovery checkpoint in this case.
    gpu_device_lost: bool,
    /// The speed test window ("速さをはかる…").
    pub bench: crate::bench::Job,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Shape,
    Material,
    Melt,
    Room,
}

/// A picture of the viewport that is waiting for the screen to be read back.
pub struct Capture {
    pub path: PathBuf,
    pub frames: u32,
}

impl App {
    pub fn new(srgb: bool, open: Option<PathBuf>) -> App {
        let scene = Scene::default();
        let mut app = App {
            committed: scene.clone(),
            tracked: scene.clone(),
            scene_rev: 1,
            committed_rev: 1,
            deduped_rev: 0,
            document_generation: 0,
            field_cache: std::cell::RefCell::new(None),
            field_builds: std::cell::Cell::new(0),
            scene,
            undo: Vec::new(),
            redo: Vec::new(),
            cam: Camera::default(),
            sel: Some(Pick::Shape(0, 0)),
            multi: Vec::new(),
            multi_for: None,
            tool: Tool::All,
            new_op: Op::Add,
            drag: None,
            srgb,
            view_mode: crate::view_mode::ViewMode::Normal,
            last_view_height: 600.0,
            path: None,
            doc_title: String::new(),
            title_edit: None,
            dirty: false,
            recovery: crate::recovery::Store::new(crate::recovery::default_directory(), std::time::Instant::now()),
            pending_file_action: None,
            native_job: None,
            queued_open: None,
            after_save: None,
            close_approved: false,
            status: (String::new(), 0.0),
            renaming: None,
            marquee_from: None,
            marquee_to: egui::Pos2::ZERO,
            input_dev: crate::view::InputDev::Auto,
            solid_mode: false,
            input_seen: crate::view::Device::Mouse,
            folded: HashSet::new(),
            layer_drag: None,
            row_rects: Vec::new(),
            shot: std::env::var_os("MELTCAD_TEST_DIR").map(PathBuf::from),
            view_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            capture: None,
            control: None,
            render_job: None,
            an: Default::default(),
            precise: None,
            precise_services: None,
            precise_prefs: None,
            precise_on: false,
            hover: None,
            fly: None,
            orbiting: false,
            show_keys: false,
            prefs: crate::palette_prefs::Prefs::load(),
            preferences: crate::prefs::Preferences::default(),
            tab: Tab::Shape,
            mat_cat: Some(crate::materials::Category::Basic),
            mat_group: false,
            mat_sort: false,
            ct: Default::default(),
            pt: Default::default(),
            sk: Default::default(),
            guide: crate::guide::Guide::load(open.is_some()),
            perf: crate::perf::Perf {
                sharpen: crate::perf::DEFAULT_SHARPEN,
                hud: std::env::var_os("MELTCAD_HUD").is_some_and(|v| v != "0"),
                ..Default::default()
            },
            clipboard: None,
            lib_open: false,
            lib_cat: 0,
            lib_query: String::new(),
            font_job: None,
            notice: None,
            melt_preview: None,
            gpu: Default::default(),
            gpu_device_lost: false,
            bench: Default::default(),
        };
        if let Some(p) = open {
            app.open_path(p);
        }
        app.apply_preferences();
        app
    }

    /// Read the remembered preferences from eframe storage (called once, when the window opens).
    pub fn load_preferences(&mut self, storage: Option<&dyn eframe::Storage>) {
        self.preferences = crate::prefs::Preferences::load_storage(storage);
        // The catalog is thread-local, so choose it here, on the UI thread, before the first frame.
        crate::i18n::set_language(&self.preferences.lang);
        self.apply_preferences();
    }

    /// Make the live preferences safe and publish the language to the translation hook.
    /// Settings call this after every change, so autosave and the UI follow immediately.
    pub fn apply_preferences(&mut self) {
        self.preferences.sanitize();
        crate::i18n::set_language(&self.preferences.lang);
        self.recovery.set_interval(self.preferences.autosave(), std::time::Instant::now());
    }

    pub fn attach_font_job(&mut self, job: crate::fonts::FontJob) {
        if job.pending() {
            self.font_job = Some(job);
        }
    }

    fn poll_font(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.font_job else { return };
        if let Some(bytes) = job.poll() {
            crate::fonts::install(ctx, bytes);
            crate::perf::mark("font installed without blocking startup");
            self.font_job = None;
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    /// The field for picking. It is built when asked for, and again only after the scene changed
    /// (a drag or a camera move that picks nothing never builds it).
    pub fn field(&self) -> std::rc::Rc<Field> {
        let mut cache = self.field_cache.borrow_mut();
        if let Some((rev, f, _)) = cache.as_ref()
            && *rev == self.scene_rev
        {
            return f.clone();
        }
        self.field_builds.set(self.field_builds.get() + 1);
        // Keep only the most recent snapshot. Shape equality guards against stale cached SDFs
        // after edits, linked-copy changes, undo/redo and load (no probabilistic hash keys).
        let f = std::rc::Rc::new(Field::reusing(&self.scene, cache.as_ref().map(|(_, field, old_scene)| (field.as_ref(), old_scene))));
        *cache = Some((self.scene_rev, f.clone(), self.scene.clone()));
        f
    }

    /// Notice a change of the scene: compares with the copy kept from the last look, and when it
    /// differs bumps the revision number. Returns the number.
    pub(crate) fn track_scene(&mut self) -> u64 {
        self.sync_multi_selection();
        if self.scene != self.tracked {
            self.tracked.clone_from(&self.scene);
            self.scene_rev += 1;
        }
        self.scene_rev
    }

    pub fn say(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.status = (text.into(), ctx.input(|i| i.time));
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.recovery.pending_path().map(std::path::Path::to_path_buf) else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::new("yuracad_recovery")).show(ctx, |ui| {
            ui.heading(i18n::tr("dialog.recovery.title"));
            ui.label(format!("{}{}", i18n::tr("dialog.recovery.data"), path.display()));
            ui.label(i18n::tr("dialog.recovery.note"));
            if self.dirty {
                ui.label(i18n::tr("dialog.recovery.dirty"));
            }
            ui.horizontal(|ui| {
                // restoring replaces what is on screen: never over work that is not saved yet
                let blocked = self.dirty;
                if ui
                    .add_enabled(!blocked, egui::Button::new(i18n::tr("dialog.recovery.restore")))
                    .on_disabled_hover_text(i18n::tr("dialog.recovery.blocked"))
                    .clicked()
                {
                    choice = Some(0);
                }
                if ui.button(i18n::tr("dialog.recovery.later")).clicked() {
                    choice = Some(1);
                }
                if ui.button(i18n::tr("dialog.recovery.delete")).clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => match self.recovery.recover_pending() {
                Ok((scene, notes, _source)) => {
                    self.clear_document_gestures();
                    self.scene = scene.clone();
                    self.committed = scene;
                    self.tracked = self.scene.clone();
                    self.scene_rev = self.scene_rev.wrapping_add(1);
                    self.committed_rev = self.scene_rev;
                    self.deduped_rev = 0;
                    self.undo.clear();
                    self.redo.clear();
                    self.path = None;
                    self.dirty = true;
                    self.sel = None;
                    self.frame_now();
                    self.say(
                        ctx,
                        if notes.is_empty() {
                            i18n::tr("dialog.recovery.restored").to_string()
                        } else {
                            format!("{}{}", i18n::tr("dialog.recovery.restored_prefix"), notes.join(", "))
                        },
                    );
                }
                Err(e) => {
                    self.say(ctx, format!("{}{e}", i18n::tr("dialog.recovery.damaged")));
                    self.recovery.postpone();
                }
            },
            Some(1) => self.recovery.postpone(),
            Some(2) => {
                if let Err(e) = self.recovery.discard_pending() {
                    self.say(ctx, format!("{}{e}", i18n::tr("dialog.recovery.delete_failed")));
                }
            }
            _ => {}
        }
    }

    // ----- editing -----

    /// Undo steps are made here: a change counts as finished once the mouse is up.
    pub(crate) fn settle(&mut self, ctx: &egui::Context) {
        let mut rev = self.track_scene();
        // a copied shape or group carries its original's number: only the first keeps it
        if self.deduped_rev != rev {
            self.scene.dedupe_uids();
            rev = self.track_scene();
            self.deduped_rev = rev;
        }
        // a live sketch solid is one undo step, made when it is accepted
        self.live_validate();
        if self.sk.live.is_some() {
            return;
        }
        if self.committed_rev != rev && !ctx.input(|i| i.pointer.any_down()) {
            // (compared exactly once per change: a drag that ends where it began is no edit)
            if self.scene != self.committed {
                self.undo.push(std::mem::replace(&mut self.committed, self.scene.clone()));
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
            }
            self.committed_rev = rev;
        }
    }

    pub(crate) fn fix_selection(&mut self) {
        let ok = match self.sel {
            Some(Pick::Shape(g, s)) => self.scene.shape(g, s).is_some(),
            Some(Pick::Group(g)) => g < self.scene.groups.len(),
            None => true,
        };
        if !ok {
            self.sel = None;
        }
        self.sync_multi_selection();
    }

    /// Draft commands keep their source indices until they finish. The older
    /// in-place outline editor can end safely before another scene command starts.
    pub(crate) fn scene_structure_locked(&self) -> bool {
        self.sk.live.is_some() || self.sk.source_sketch.is_some() || self.sk.guide_edit.is_some()
    }

    pub(crate) fn allow_scene_structure_change(&mut self) -> bool {
        if self.scene_structure_locked() {
            self.sk.msg = i18n::tr("error.active_sketch").into();
            false
        } else {
            if self.sk.edit.is_some() {
                self.sketch_off();
            }
            true
        }
    }

    /// Continue an in-place outline edit only across changes to that exact
    /// outline. Old scenes may have no UIDs, so matching kind and index alone
    /// would accidentally bind the editor to another shape after reordering.
    fn prepare_outline_history(&mut self, next: &Scene) {
        let Some(edit) = self.sk.edit.as_ref() else { return };
        let (g, s, which) = (edit.g, edit.s, edit.which);
        let keep = next.shape(g, s).filter(|sh| crate::sketch::edit_frame(sh, which).is_some()).is_some_and(|after| {
            let mut comparable = self.scene.clone();
            let Some(before) = comparable.shape_mut(g, s) else { return false };
            before.outline.clone_from(&after.outline);
            before.outline2.clone_from(&after.outline2);
            comparable == *next
        });
        if keep {
            if let Some(edit) = self.sk.edit.as_mut() {
                edit.sel = None;
                edit.drag = None;
                edit.hover = None;
                edit.edge_sel = None;
            }
            self.sk.edit_op = None;
            self.sk.edit_index = None;
            self.sk.snap_hit = None;
            self.sk.msg.clear();
        } else {
            self.sketch_off();
        }
    }

    /// Take the scene as the finished state (no undo step), after the program itself changed it.
    pub(crate) fn committed_sync(&mut self) {
        self.committed = self.scene.clone();
    }

    pub fn undo(&mut self) {
        if self.sk.live.is_some() {
            // undo while shaping = throw the live solid away
            self.live_discard();
            return;
        }
        if self.scene_structure_locked() {
            self.allow_scene_structure_change();
            return;
        }
        if self.edit_cancel_drag() {
            return;
        }
        if let Some(prev) = self.undo.pop() {
            self.prepare_outline_history(&prev);
            self.redo.push(std::mem::replace(&mut self.scene, prev.clone()));
            self.committed = prev;
            // The document no longer matches what was last saved (an undo after a save used to
            // stay "clean", so closing threw the undone state away without asking).
            self.dirty = true;
            self.fix_selection();
            self.animation_at_applied(self.an.frame);
            // Invalidate both CPU field and GPU pack before any further hit test/draw.
            self.track_scene();
            self.committed_rev = self.scene_rev;
        }
    }

    #[cfg(test)]
    pub(crate) fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo(&mut self) {
        if self.scene_structure_locked() {
            self.allow_scene_structure_change();
            return;
        }
        if self.edit_cancel_drag() {
            return;
        }
        if let Some(next) = self.redo.pop() {
            self.prepare_outline_history(&next);
            self.undo.push(std::mem::replace(&mut self.scene, next.clone()));
            self.committed = next;
            // A redo is a document change too: never leave a redone state looking saved.
            self.dirty = true;
            self.fix_selection();
            self.animation_at_applied(self.an.frame);
            self.track_scene();
            self.committed_rev = self.scene_rev;
        }
    }

    pub fn new_scene(&mut self, scene: Scene) {
        self.request_file_action(PendingFileAction::New(scene));
    }

    fn has_unsaved_work(&self) -> bool {
        self.dirty || self.scene != self.committed || self.sketch_has_draft() || (self.sk.on && self.sk.separate)
    }

    fn sketch_has_draft(&self) -> bool {
        self.sk.on
            && self.sk.keep_sketch
            && (!self.sk.pts.is_empty() || self.sk.anchor.is_some() || !self.sk.profile.is_empty() || self.sk.stage == crate::sketch_ui::Stage::Ready)
    }

    /// Indices in a gesture belong to one document. Forget them before replacing
    /// the scene so Esc/drag release can never edit a similarly indexed new shape.
    fn clear_document_gestures(&mut self) {
        self.sk.reset();
        self.sk.on = false;
        self.sk.edit = None;
        self.sk.edit_op = None;
        self.sk.edit_index = None;
        self.ct = crate::curve_tool::State::default();
        self.pt = crate::pipe_tool::State::default();
        self.drag = None;
        self.marquee_from = None;
        self.layer_drag = None;
        self.renaming = None;
        self.multi.clear();
        self.multi_for = None;
        self.hover = None;
        self.orbiting = false;
        self.capture = None;
        self.fly = None;
    }

    fn request_file_action(&mut self, action: PendingFileAction) {
        if self.pending_file_action.is_some() || self.native_job.is_some() || self.queued_open.is_some() || self.after_save.is_some() {
            self.notice = Some(i18n::tr("error.wait_for_job").into());
            return;
        }
        if self.has_unsaved_work() {
            self.pending_file_action = Some(action);
            self.capture = None;
        } else {
            self.apply_file_action(action);
        }
    }

    fn apply_file_action(&mut self, action: PendingFileAction) {
        match action {
            PendingFileAction::New(scene) => {
                self.install_scene(scene, None, Vec::new());
                self.guide.new_scene_applied();
            }
            PendingFileAction::Open(path) => {
                // No file I/O or JSON parsing on the UI thread. The current design stays
                // visible until the worker reports a fully validated new Scene.
                self.queued_open = Some(QueuedOpen { path, base_revision: self.track_scene() });
            }
            PendingFileAction::Close => {}
        }
    }

    fn install_scene(&mut self, scene: Scene, path: Option<PathBuf>, notes: Vec<String>) {
        if let Err(e) = self.recovery.clear_active() {
            self.notice = Some(format!("{}{e}", i18n::tr("error.recovery_cleanup")));
        }
        self.clear_document_gestures();
        self.document_generation = self.document_generation.wrapping_add(1);
        self.scene = scene.clone();
        self.committed = scene;
        self.undo.clear();
        self.redo.clear();
        self.sel = if self.scene.shape(0, 0).is_some() { Some(Pick::Shape(0, 0)) } else { None };
        self.path = path;
        // a new or opened document does not inherit the last one's typed title
        self.doc_title.clear();
        self.title_edit = None;
        self.dirty = false;
        // A different document must never inherit the old timeline's frame,
        // playback state or edit revision. Otherwise the next UI frame can
        // auto-record the new scene over one of its saved animation keys.
        self.animation_at_applied(0);
        if !notes.is_empty() {
            self.notice = Some(notes.join(", "));
        }
        self.frame_now();
    }

    fn resolve_pending_file_action(&mut self, ctx: &egui::Context, decision: PendingFileDecision) {
        let Some(action) = self.pending_file_action.take() else { return };
        match decision {
            PendingFileDecision::Cancel => {}
            PendingFileDecision::Discard => self.finish_pending_file_action(ctx, action),
            PendingFileDecision::Save => {
                if self.try_save(ctx, false) {
                    self.after_save = Some(action);
                } else {
                    self.pending_file_action = Some(action);
                }
            }
        }
    }

    fn finish_pending_file_action(&mut self, ctx: &egui::Context, action: PendingFileAction) {
        if matches!(action, PendingFileAction::Close) {
            self.close_approved = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.apply_file_action(action);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.close_approved
            && (self.native_job.is_some()
                || self.queued_open.is_some()
                || self.after_save.is_some()
                || self.render_job.is_some()
                || self.an.gif_running()
                || crate::work_budget::is_busy(ctx))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.say(ctx, i18n::tr("error.close_file_job"));
            return;
        }
        if self.precise.as_ref().is_some_and(|p| p.app.quit_requested) {
            if let Some(p) = self.precise.as_mut() {
                p.app.quit_requested = false;
            }
            self.precise_on = false;
            style::apply(ctx);
            if self.has_unsaved_work() {
                self.pending_file_action = Some(PendingFileAction::Close);
            } else {
                self.close_approved = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved {
            if self.precise_transfer_pending() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.say(ctx, i18n::tr("error.precise_transfer"));
                return;
            }
            if let Some(p) = self.precise.as_mut()
                && solidcraft_ui_egui::documents::any_dirty(&p.app)
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                solidcraft_ui_egui::documents::request_quit(&mut p.app);
                self.precise_on = true;
                solidcraft_ui_egui::theme::apply(ctx);
                ctx.request_repaint();
                return;
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved && (self.has_unsaved_work() || self.pending_file_action.is_some()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.precise_on = false;
            style::apply(ctx);
            if self.pending_file_action.is_none() {
                self.pending_file_action = Some(PendingFileAction::Close);
            }
        }
    }

    fn pending_file_dialog(&mut self, ctx: &egui::Context) {
        if self.pending_file_action.is_none() {
            return;
        }
        let mut decision = None;
        egui::Modal::new(egui::Id::new("unsaved_file_action")).show(ctx, |ui| {
            ui.heading(i18n::tr("dialog.unsaved.title"));
            ui.label(i18n::tr("dialog.unsaved.body"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(i18n::tr("dialog.unsaved.save")).clicked() {
                    decision = Some(PendingFileDecision::Save);
                }
                if ui.button(i18n::tr("dialog.unsaved.discard")).clicked() {
                    decision = Some(PendingFileDecision::Discard);
                }
                if ui.button(i18n::tr("dialog.unsaved.cancel")).clicked() {
                    decision = Some(PendingFileDecision::Cancel);
                }
            });
        });
        if let Some(decision) = decision {
            self.resolve_pending_file_action(ctx, decision);
        }
    }

    /// The group a new shape goes into (made when there is none).
    pub(crate) fn target_group(&mut self) -> Option<usize> {
        let g = match self.sel {
            Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) => Some(g),
            None => self.scene.groups.len().checked_sub(1),
        };
        match g {
            // Linked copies own no geometry. Putting a shape here would be invisible
            // and Scene::sanitize would silently delete it on reload.
            Some(g) if self.scene.groups.get(g).is_some_and(Group::is_linked) => None,
            Some(g) if g < self.scene.groups.len() => Some(g),
            _ => {
                let name = self.scene.unique_group_name("Group 1");
                self.scene.groups.push(Group::new(name));
                Some(self.scene.groups.len() - 1)
            }
        }
    }

    pub fn add_shape(&mut self, ctx: &egui::Context, kind: Kind) {
        if !self.allow_scene_structure_change() {
            return;
        }
        if self.scene.shape_count() >= MAX_SHAPES {
            self.say(ctx, format!("Up to {MAX_SHAPES} shapes"));
            return;
        }
        let Some(g) = self.target_group() else {
            self.say(ctx, "This is a linked copy. Select its source or Unlink it before adding a shape");
            return;
        };
        // beside the selected shape so the two melt together; else at the view's centre
        let size = kind.default_size();
        let rest = match kind {
            Kind::Sphere => size[0],
            Kind::Box => size[2] * 0.5,
            Kind::Cylinder | Kind::Cone => size[1] * 0.5,
            Kind::Torus => size[1],
            Kind::Extrude => size[0] * 0.5,
            _ => Shape::new(kind, [0.0; 3], Material::default()).local_half()[2],
        };
        let pos = match self.sel.and_then(|p| centre_of(&self.scene, Some(p))) {
            Some(c) if self.scene.shape_count() > 0 => [c[0] + 18.0, c[1] + 6.0, c[2].max(rest) + 4.0],
            _ => [self.cam.target[0], self.cam.target[1], rest],
        };
        let mut sh = Shape::new(kind, pos, self.next_material());
        sh.op = self.new_op;
        if self.new_op == Op::Subtract {
            sh.material = Material::plain([0.9, 0.35, 0.4], 0.6, 0.0);
        }
        let n = self.scene.groups[g].shapes.len();
        self.scene.groups[g].shapes.push(sh);
        self.sel = Some(Pick::Shape(g, n));
    }

    fn next_material(&self) -> Material {
        const COLORS: [[f32; 3]; 6] =
            [[0.93, 0.56, 0.45], [0.60, 0.72, 0.98], [0.62, 0.90, 0.74], [0.96, 0.80, 0.45], [0.76, 0.66, 0.96], [0.96, 0.66, 0.82]];
        Material { color: COLORS[self.scene.shape_count() % COLORS.len()], ..Material::default() }
    }

    pub fn add_group(&mut self) {
        if !self.allow_scene_structure_change() {
            return;
        }
        if self.scene.groups.len() < MAX_GROUPS {
            let n = self.scene.groups.len() + 1;
            let name = self.scene.unique_group_name(&format!("Group {n}"));
            self.scene.groups.push(Group::new(name));
            self.sel = Some(Pick::Group(self.scene.groups.len() - 1));
        }
    }

    pub fn duplicate(&mut self) {
        if !self.multi.is_empty() {
            self.duplicate_multi();
            return;
        }
        if !self.allow_scene_structure_change() {
            return;
        }
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape_count() < MAX_SHAPES => {
                if let Some(mut c) = self.scene.shape(g, s).cloned() {
                    if let Err(why) = crate::mesh::can_add(&self.scene, std::iter::once(&c)) {
                        self.notice = Some(why.into());
                        return;
                    }
                    c.name = format!("{} copy", c.name);
                    self.scene.groups[g].shapes.insert(s + 1, c);
                    self.sel = Some(Pick::Shape(g, s + 1));
                }
            }
            Some(Pick::Group(g)) if g < self.scene.groups.len() => {
                let e = self.scene.subtree_end(g);
                let copies: Vec<Group> = self.scene.groups[g..e].to_vec();
                let shapes: usize = copies.iter().map(|c| c.shapes.len()).sum();
                if self.scene.groups.len() + copies.len() > MAX_GROUPS || self.scene.shape_count() + shapes > MAX_SHAPES {
                    return;
                }
                if let Err(why) = crate::mesh::can_add(&self.scene, copies.iter().flat_map(|c| c.shapes.iter())) {
                    self.notice = Some(why.into());
                    return;
                }
                // the copy is a set of its own: links and patterns inside it follow the copy
                let mut copies = copies;
                let mut next = self.scene.next_uid();
                Scene::renumber_copies(&mut next, &mut copies);
                for (i, mut c) in copies.into_iter().enumerate() {
                    if i == 0 {
                        c.name = format!("{} copy", c.name);
                    }
                    // a copy of one letter is a plain solid, not a second owner of the text's identity
                    c.puffy_text = None;
                    self.scene.groups.insert(e + i, c);
                }
                self.sel = Some(Pick::Group(e));
            }
            _ => {}
        }
    }

    /// Copy the selected shape to the other side of the YZ plane (x becomes -x): handy for
    /// ears, arms and eyes.
    pub fn mirror_copy(&mut self) {
        if !self.allow_scene_structure_change() {
            return;
        }
        let Some(Pick::Shape(g, s)) = self.sel else { return };
        if self.scene.shape_count() >= MAX_SHAPES {
            return;
        }
        if let Some(mut c) = self.scene.shape(g, s).cloned() {
            if let Err(why) = crate::mesh::can_add(&self.scene, std::iter::once(&c)) {
                self.notice = Some(why.into());
                return;
            }
            c.pos[0] = -c.pos[0];
            c.rot = [c.rot[0], -c.rot[1], -c.rot[2]];
            for p in &mut c.outline {
                p[0] = -p[0];
            }
            c.outline.reverse();
            // Reflect every representation of the model, not just the main
            // 2D outline. Curve/Sweep use path nodes and optional Bezier
            // handles; Loft uses an independent upper outline.
            for p in &mut c.outline2 {
                p[0] = -p[0];
            }
            c.outline2.reverse();
            for p in &mut c.path {
                p[0] = -p[0];
            }
            for h in &mut c.curve_handles {
                h.incoming[0] = -h.incoming[0];
                h.outgoing[0] = -h.outgoing[0];
            }
            if let Some(plane) = &mut c.guide_plane {
                for hole in &mut plane.holes {
                    for p in hole.iter_mut() {
                        p[0] = -p[0];
                    }
                    hole.reverse();
                }
            }
            c.uid = 0; // The mirrored copy needs its own animation/pattern identity.
            c.name = format!("{} (mirror)", c.name);
            self.scene.groups[g].shapes.insert(s + 1, c);
            self.sel = Some(Pick::Shape(g, s + 1));
        }
    }

    /// Everything selected: `sel` first, then any transient extra picks.
    pub fn picks(&self) -> Vec<Pick> {
        let mut v: Vec<Pick> = self.sel.into_iter().collect();
        if self.sel.is_some() && self.multi_for == self.sel {
            v.extend(self.multi.iter().copied().filter(|p| Some(*p) != self.sel));
        }
        v
    }

    /// The shapes and groups a "Melt" press is about: everything picked, and the groups of picked shapes.
    fn melt_targets(&self) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
        let picks = self.picks();
        let mut shapes: Vec<(usize, usize)> = Vec::new();
        let mut picked_groups: Vec<usize> = Vec::new();
        let mut parents: Vec<usize> = Vec::new();
        for p in &picks {
            match *p {
                Pick::Shape(g, s) => {
                    shapes.push((g, s));
                    parents.push(g);
                }
                Pick::Group(g) => picked_groups.push(g),
            }
        }
        parents.sort_unstable();
        parents.dedup();
        (shapes, picked_groups, parents)
    }

    /// Is everything picked set to melt (shapes and the groups they sit in)?
    pub fn selection_melts(&self) -> bool {
        let (shapes, groups, parents) = self.melt_targets();
        if shapes.is_empty() && groups.is_empty() {
            return false;
        }
        let _ = parents;
        shapes.iter().all(|&(g, s)| self.scene.shape(g, s).is_some_and(|x| x.blend > 0.0))
            && groups.iter().all(|&g| self.scene.groups.get(g).is_some_and(|x| x.blend > 0.0))
    }

    /// One press: melt what is picked into its neighbours (the shapes AND the groups, which used to
    /// be two switches), or take the melt away again. With nothing picked, it is the whole scene.
    pub fn toggle_melt_selected(&mut self) {
        let (mut shapes, groups, mut parents) = self.melt_targets();
        if shapes.is_empty() && groups.is_empty() {
            // the whole scene: the shapes inside their groups only; different groups stay apart
            // unless a group row is picked and melted on purpose
            shapes = (0..self.scene.groups.len()).flat_map(|g| (0..self.scene.groups[g].shapes.len()).map(move |s| (g, s))).collect();
            parents.clear();
        }
        let on = !self.selection_melts_of(&shapes, &groups, &parents);
        for &(g, s) in &shapes {
            if let Some(sh) = self.scene.shape_mut(g, s) {
                sh.blend = if on { if sh.blend > 0.0 { sh.blend } else { 6.0 } } else { 0.0 };
            }
        }
        // Turning off: a parent group whose shapes are all being turned off stops melting too, so
        // "off" really stops the melt into the other groups (#236). A parent that still holds a
        // melting shape keeps its melt, so that shape stays as it was.
        let emptied: Vec<usize> = if on {
            Vec::new()
        } else {
            parents
                .iter()
                .copied()
                .filter(|&g| {
                    self.scene.groups.get(g).is_some_and(|gr| (0..gr.shapes.len()).all(|s| shapes.contains(&(g, s)) || gr.shapes[s].blend <= 0.0))
                })
                .collect()
        };
        // (turning melt on for a shape never makes its group melt into the other groups)
        for &g in groups.iter().chain(emptied.iter()) {
            if let Some(gr) = self.scene.groups.get_mut(g) {
                gr.blend = if on { if gr.blend > 0.0 { gr.blend } else { 12.0 } } else { 0.0 };
            }
        }
    }

    fn selection_melts_of(&self, shapes: &[(usize, usize)], groups: &[usize], parents: &[usize]) -> bool {
        let _ = parents;
        !(shapes.is_empty() && groups.is_empty())
            && shapes.iter().all(|&(g, s)| self.scene.shape(g, s).is_some_and(|x| x.blend > 0.0))
            && groups.iter().all(|&g| self.scene.groups.get(g).is_some_and(|x| x.blend > 0.0))
    }

    /// The shapes of everything that is selected, each once.
    pub fn sel_targets(&self) -> Vec<(usize, usize)> {
        crate::view::targets_all(&self.scene, &self.picks())
    }

    /// Shift + click: add `p` to the selection, or take it out again.
    pub fn toggle_pick(&mut self, p: Pick) {
        if self.multi_for != self.sel {
            self.multi.clear();
        }
        match self.sel {
            None => self.sel = Some(p),
            Some(s) if s == p => {
                self.sel = if self.multi.is_empty() { None } else { Some(self.multi.remove(0)) };
            }
            Some(_) => {
                if let Some(i) = self.multi.iter().position(|m| *m == p) {
                    self.multi.remove(i);
                } else {
                    self.multi.push(p);
                }
            }
        }
        self.multi_for = self.sel;
    }

    /// Put the selected shapes into one new group (they melt together; the group is the unit to move and melt with others).
    pub fn group_selected(&mut self) {
        if !self.allow_scene_structure_change() {
            return;
        }
        let picked_groups = self.picks().iter().any(|p| matches!(p, Pick::Group(_)));
        if picked_groups {
            self.group_groups();
            return;
        }
        let mut shapes: Vec<(usize, usize)> =
            self.picks().into_iter().filter_map(|p| if let Pick::Shape(g, s) = p { Some((g, s)) } else { None }).collect();
        if shapes.len() < 2 || self.scene.groups.len() >= MAX_GROUPS {
            return;
        }
        shapes.sort();
        shapes.dedup();
        let base = format!("Group {}", self.scene.groups.len() + 1);
        let mut group = Group::new(self.scene.unique_group_name(&base));
        let mut taken: Vec<Shape> = Vec::new();
        for (g, s) in shapes.iter().rev() {
            if let Some(sh) = self.scene.groups.get_mut(*g).filter(|gr| *s < gr.shapes.len()).map(|gr| gr.shapes.remove(*s)) {
                taken.push(sh);
            }
        }
        taken.reverse();
        group.shapes = taken;
        let at = self.scene.groups.len();
        self.scene.groups.push(group);
        self.folded.clear();
        self.multi.clear();
        self.sel = Some(Pick::Group(at));
        self.multi_for = self.sel;
    }

    /// Put the picked groups (and any picked loose shapes) inside one new parent group, so groups
    /// can be nested (a "big group"). One picked group gets wrapped on its own.
    fn group_groups(&mut self) {
        let n = self.scene.groups.len();
        if n >= MAX_GROUPS {
            return;
        }
        let picks = self.picks();
        let mut groups: Vec<usize> = picks.iter().filter_map(|p| if let Pick::Group(g) = p { Some(*g) } else { None }).filter(|g| *g < n).collect();
        groups.sort_unstable();
        groups.dedup();
        // only the outermost picked groups: what sits inside one of them travels with it
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for &g in &groups {
            if !ranges.iter().any(|&(a, b)| g >= a && g < b) {
                ranges.push((g, self.scene.subtree_end(g)));
            }
        }
        let Some(&(first, _)) = ranges.first() else { return };
        let mut loose: Vec<(usize, usize)> = picks
            .iter()
            .filter_map(|p| if let Pick::Shape(g, s) = p { Some((*g, *s)) } else { None })
            .filter(|(g, _)| !ranges.iter().any(|&(a, b)| *g >= a && *g < b))
            .collect();
        loose.sort_unstable();
        loose.dedup();
        // a group of groups gets a name that says so, numbered among the big groups
        let bigs = (0..n).filter(|&g| self.scene.groups.get(g + 1).is_some_and(|x| x.depth > self.scene.groups[g].depth)).count();
        let base = format!("Parent Group {}", bigs + 1);
        let mut parent = Group::new(self.scene.unique_group_name(&base));
        let mut taken: Vec<Shape> = Vec::new();
        for (g, s) in loose.iter().rev() {
            if let Some(sh) = self.scene.groups.get_mut(*g).filter(|gr| *s < gr.shapes.len()).map(|gr| gr.shapes.remove(*s)) {
                taken.push(sh);
            }
        }
        taken.reverse();
        parent.shapes = taken;
        let pdepth = self.scene.groups.get(first).map_or(0, |g| g.depth);
        parent.depth = pdepth;
        let tdepth: Vec<i32> = ranges.iter().map(|&(a, _)| self.scene.groups.get(a).map_or(0, |g| i32::from(g.depth))).collect();
        let old = std::mem::take(&mut self.scene.groups);
        let mut out: Vec<Group> = Vec::with_capacity(old.len() + 1);
        let mut block: Vec<Group> = Vec::new();
        for (i, mut g) in old.into_iter().enumerate() {
            if i == first {
                out.push(std::mem::replace(&mut parent, Group::new(String::new())));
            }
            if let Some(k) = ranges.iter().position(|&(a, b)| i >= a && i < b) {
                let base = tdepth.get(k).copied().unwrap_or(0);
                g.depth = (i32::from(g.depth) - base + i32::from(pdepth) + 1).clamp(0, i32::from(crate::model::MAX_DEPTH)) as u8;
                block.push(g);
            } else {
                out.push(g);
            }
        }
        let at = (first + 1).min(out.len());
        for (i, g) in block.into_iter().enumerate() {
            out.insert(at + i, g);
        }
        self.scene.groups = out;
        self.scene.normalize_depths();
        self.folded.clear();
        self.multi.clear();
        self.sel = Some(Pick::Group(first));
        self.multi_for = self.sel;
    }

    pub fn delete_selected(&mut self) {
        if !self.allow_scene_structure_change() {
            return;
        }
        let picks = self.picks();
        if picks.len() > 1 {
            let mut shapes: Vec<(usize, usize)> =
                picks.iter().filter_map(|p| if let Pick::Shape(g, s) = p { Some((*g, *s)) } else { None }).collect();
            let mut groups: Vec<usize> = picks.iter().filter_map(|p| if let Pick::Group(g) = p { Some(*g) } else { None }).collect();
            shapes.sort();
            shapes.dedup();
            groups.sort();
            groups.dedup();
            for (g, s) in shapes.into_iter().rev() {
                if let Some(gr) = self.scene.groups.get_mut(g).filter(|gr| s < gr.shapes.len()) {
                    gr.shapes.remove(s);
                }
            }
            for g in groups.into_iter().rev() {
                self.scene.remove_group(g);
            }
            self.folded.clear();
            self.multi.clear();
            self.sel = None;
            return;
        }
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape(g, s).is_some() => {
                self.scene.groups[g].shapes.remove(s);
                self.sel = if self.scene.groups[g].shapes.is_empty() {
                    Some(Pick::Group(g))
                } else {
                    Some(Pick::Shape(g, s.min(self.scene.groups[g].shapes.len() - 1)))
                };
            }
            Some(Pick::Group(g)) if g < self.scene.groups.len() => {
                self.scene.remove_group(g);
                self.folded.clear();
                self.sel = None;
            }
            _ => {}
        }
    }

    /// Drag-and-drop in the scene list: put `from` next to `to` (before it, or after it).
    pub fn move_layer(&mut self, from: Pick, to: Pick, after: bool) {
        if !self.allow_scene_structure_change() {
            self.layer_drag = None;
            return;
        }
        match (from, to) {
            (Pick::Shape(g, s), Pick::Shape(g2, s2)) => {
                if self.scene.shape(g, s).is_none() || self.scene.shape(g2, s2).is_none() || (g, s) == (g2, s2) {
                    return;
                }
                let sh = self.scene.groups[g].shapes.remove(s);
                let mut idx = if g == g2 && s < s2 { s2 - 1 } else { s2 };
                if after {
                    idx += 1;
                }
                let idx = idx.min(self.scene.groups[g2].shapes.len());
                self.scene.groups[g2].shapes.insert(idx, sh);
                self.sel = Some(Pick::Shape(g2, idx));
            }
            (Pick::Shape(g, s), Pick::Group(g2)) => {
                if self.scene.shape(g, s).is_none() || g2 >= self.scene.groups.len() {
                    return;
                }
                // A loose shape lives in a group. Dropped on the line above a group row it joins the
                // group that ends there (the one above, #279); below the row it becomes that group's first shape.
                let sh = self.scene.groups[g].shapes.remove(s);
                if after {
                    self.scene.groups[g2].shapes.insert(0, sh);
                    self.sel = Some(Pick::Shape(g2, 0));
                } else {
                    let target = g2.saturating_sub(1);
                    self.scene.groups[target].shapes.push(sh);
                    self.sel = Some(Pick::Shape(target, self.scene.groups[target].shapes.len() - 1));
                }
            }
            (Pick::Group(g), Pick::Group(g2) | Pick::Shape(g2, _)) => {
                if g >= self.scene.groups.len() || g2 >= self.scene.groups.len() || g == g2 {
                    return;
                }
                let e = self.scene.subtree_end(g);
                if (g..e).contains(&g2) {
                    return;
                }
                // a subtree that would not fit at the target's depth is refused, not flattened (#203)
                let base = self.scene.groups[g].depth;
                let deepest = self.scene.groups[g..e].iter().map(|x| x.depth - base).max().unwrap_or(0);
                if self.scene.groups[g2].depth.saturating_add(deepest) > crate::model::MAX_DEPTH {
                    self.notice = Some("A Parent Group cannot go inside another group".into());
                    return;
                }
                // (the place among the groups that stay, and the depth of the group it lands beside)
                let removed = e - g;
                let t = if g2 >= e { g2 - removed } else { g2 };
                let (tdepth, tend) = (self.scene.groups[g2].depth, self.scene.subtree_end(g2));
                let idx = if after { (if tend >= e { tend - removed } else { tend }).max(t + 1) } else { t };
                let idx = self.scene.move_subtree(g, idx, tdepth);
                self.folded.clear();
                self.sel = Some(Pick::Group(idx));
            }
        }
    }

    /// A row dropped onto the middle of a group's row: it goes inside that group.
    pub fn move_layer_into(&mut self, from: Pick, into: usize) -> Result<(), &'static str> {
        if !self.allow_scene_structure_change() {
            return Err("Finish or cancel the active command first");
        }
        match from {
            Pick::Shape(g, s) => {
                if self.scene.shape(g, s).is_none() || into >= self.scene.groups.len() {
                    return Ok(());
                }
                let sh = self.scene.groups[g].shapes.remove(s);
                self.scene.groups[into].shapes.push(sh);
                self.sel = Some(Pick::Shape(into, self.scene.groups[into].shapes.len() - 1));
                Ok(())
            }
            Pick::Group(g) => {
                let at = self.scene.move_group_into(g, into).ok_or("A group cannot go inside itself, or more than 4 levels deep")?;
                self.folded.clear();
                self.multi.clear();
                self.sel = Some(Pick::Group(at));
                self.multi_for = self.sel;
                Ok(())
            }
        }
    }

    /// Several picked rows dropped inside group `into`: the loose shapes keep their order and join it,
    /// the picked groups (with what is nested in them) nest in it.
    pub fn move_picks_into(&mut self, picks: &[Pick], into: usize) -> Result<(), &'static str> {
        if !self.allow_scene_structure_change() {
            return Err("Finish or cancel the active command first");
        }
        let n = self.scene.groups.len();
        if into >= n {
            return Ok(());
        }
        let mut starts: Vec<usize> = picks.iter().filter_map(|p| if let Pick::Group(g) = p { Some(*g) } else { None }).filter(|g| *g < n).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for &g in &starts {
            if !ranges.iter().any(|&(a, b)| g >= a && g < b) {
                ranges.push((g, self.scene.subtree_end(g)));
            }
        }
        if ranges.iter().any(|&(a, b)| (a..b).contains(&into)) {
            return Err("A group cannot go inside itself, or more than 4 levels deep");
        }
        let mut shapes: Vec<(usize, usize)> = picks
            .iter()
            .filter_map(|p| if let Pick::Shape(g, s) = p { Some((*g, *s)) } else { None })
            .filter(|(g, _)| *g != into && !ranges.iter().any(|&(a, b)| *g >= a && *g < b))
            .collect();
        shapes.sort_unstable();
        shapes.dedup();
        for &(g, s) in shapes.iter().rev() {
            if self.scene.shape(g, s).is_some() {
                let sh = self.scene.groups[g].shapes.remove(s);
                self.scene.groups[into].shapes.insert(0, sh);
            }
        }
        // one group at a time; the places of the others and of the target shift after each move
        let mut pending: Vec<usize> = ranges.iter().map(|r| r.0).collect();
        let mut target = into;
        let mut result = Ok(());
        while !pending.is_empty() {
            let g = pending.remove(0);
            let e = self.scene.subtree_end(g);
            let Some(at) = self.scene.move_group_into(g, target) else {
                result = Err("A group cannot go inside itself, or more than 4 levels deep");
                break;
            };
            let remap = |i: usize| -> usize {
                if (g..e).contains(&i) {
                    return at + (i - g);
                }
                let j = if i >= e { i - (e - g) } else { i };
                if j >= at { j + (e - g) } else { j }
            };
            target = remap(target);
            for x in &mut pending {
                *x = remap(*x);
            }
        }
        self.folded.clear();
        self.multi.clear();
        self.sel = Some(Pick::Group(target.min(self.scene.groups.len().saturating_sub(1))));
        self.multi_for = self.sel;
        result
    }

    /// Move the selection by `d` millimetres.
    pub fn nudge(&mut self, d: [f32; 3]) {
        // A ball chosen in the scene list is moved on its own: its siblings, the curve through
        // them and the group's melt all stay as they are.
        if let Some((g, s, i)) = self.picked_node()
            && let Some(w) = self.node_world(g, s, i)
        {
            self.set_node_world(g, s, i, [w[0] + d[0], w[1] + d[1], w[2] + d[2]]);
            return;
        }
        let mut targets = self.sel_targets();
        let mut seen: std::collections::HashSet<(usize, usize)> = targets.iter().copied().collect();
        let mut linked = std::collections::HashSet::new();
        // Which groups are moving: a whole selected subtree and the groups of selected shapes.
        // A linked copy whose source is among them already follows it (#261).
        let mut selected_groups: std::collections::HashSet<usize> = std::collections::HashSet::new();
        // A group selection includes its descendants. The viewport's ordinary
        // targets helper only lists its own shapes; applying that here used to
        // leave nested children behind when nudging their parent.
        for pick in self.picks() {
            match pick {
                Pick::Group(g) => {
                    for gi in g..self.scene.subtree_end(g) {
                        selected_groups.insert(gi);
                        if let Some(group) = self.scene.groups.get(gi) {
                            if group.is_linked() {
                                linked.insert(gi);
                            }
                            for si in 0..group.shapes.len() {
                                if seen.insert((gi, si)) {
                                    targets.push((gi, si));
                                }
                            }
                        }
                    }
                }
                Pick::Shape(g, _) => {
                    selected_groups.insert(g);
                }
            }
        }
        for (g, s) in targets {
            if let Some(sh) = self.scene.shape_mut(g, s) {
                sh.pos = [sh.pos[0] + d[0], sh.pos[1] + d[1], sh.pos[2] + d[2]];
            }
        }
        // Linked copies have no shapes; move each transform once, including
        // links nested in selected parents or selected along with an ancestor.
        for g in linked {
            // The source is moving too, so the copy already travels with it; adding to the
            // link's own transform as well would move it twice.
            if crate::pattern::link_source(&self.scene, g).is_some_and(|src| selected_groups.contains(&src)) {
                continue;
            }
            if let Some(gr) = self.scene.groups.get_mut(g) {
                gr.xform.pos = [gr.xform.pos[0] + d[0], gr.xform.pos[1] + d[1], gr.xform.pos[2] + d[2]];
                gr.xform.sanitize();
            }
        }
    }

    /// Switch plus and minus on the selection.
    pub fn flip_op(&mut self) {
        if self.sk.live.is_some() {
            self.new_op = self.new_op.flip();
            self.live_update();
            return;
        }
        match self.sel {
            Some(Pick::Shape(g, s)) => {
                if let Some(sh) = self.scene.shape_mut(g, s) {
                    sh.op = sh.op.flip();
                }
            }
            Some(Pick::Group(g)) => {
                if let Some(gr) = self.scene.groups.get_mut(g) {
                    gr.op = gr.op.flip();
                }
            }
            None => {}
        }
    }

    // ----- files -----

    pub fn open_path(&mut self, path: PathBuf) {
        self.request_file_action(PendingFileAction::Open(path));
    }

    fn open_dialog(&mut self, _ctx: &egui::Context) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Melt 3D", &["meltcad", "yura", "json"]).pick_file() {
            self.open_path(p);
        }
    }

    /// The design's name: the file's name, else the one typed into the title, else "Untitled".
    pub(crate) fn doc_name(&self) -> String {
        if let Some(stem) = self.path.as_ref().and_then(|p| p.file_stem()) {
            return stem.to_string_lossy().into_owned();
        }
        if self.doc_title.is_empty() { "Untitled".into() } else { self.doc_title.clone() }
    }

    /// A new name typed into the title. Without a file it is only remembered (the first save
    /// offers it); with a file, the file is renamed next to where it is. Nothing is overwritten.
    fn rename_doc(&mut self, ctx: &egui::Context, typed: &str) {
        let name = clean_title(typed);
        if name.is_empty() || name == self.doc_name() {
            return;
        }
        // A save/open job was started for the old path; renaming now would let it write the old
        // name back and restore it when it finishes (#262).
        if self.native_job.is_some() || self.queued_open.is_some() || self.after_save.is_some() {
            self.say(ctx, i18n::tr("error.rename_saving"));
            return;
        }
        let Some(old) = self.path.clone() else {
            self.doc_title = name;
            return;
        };
        let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "meltcad".into());
        let new = old.with_file_name(format!("{name}.{ext}"));
        if new.exists() {
            self.say(ctx, format!("There is already a file named {name}"));
            return;
        }
        match std::fs::rename(&old, &new) {
            Ok(()) => self.path = Some(new),
            Err(e) => self.say(ctx, format!("Could not rename: {e}")),
        }
    }

    fn save(&mut self, ctx: &egui::Context, ask: bool) {
        self.try_save(ctx, ask);
    }

    fn try_save(&mut self, ctx: &egui::Context, ask: bool) -> bool {
        if self.native_job.is_some() || self.queued_open.is_some() {
            self.say(ctx, "Wait for the other file job to finish before saving");
            return false;
        }
        let pending_solid = self.sk.on && self.sk.separate;
        if (self.sketch_has_draft() || pending_solid) && (!self.sketch_confirm(ctx) || self.sketch_has_draft() || (self.sk.on && self.sk.separate)) {
            self.say(ctx, "Finish or cancel the current sketch or solid operation before saving".to_string());
            return false;
        }
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("Melt 3D", &["meltcad"]).set_file_name(format!("{}.meltcad", self.doc_name())).save_file(),
        };
        let Some(path) = path else { return false };
        let rev = self.track_scene();
        match crate::document_job::Job::save(ctx, path, self.scene.clone(), rev) {
            Ok(job) => {
                self.native_job = Some(job);
                ctx.request_repaint();
                true // started, NOT yet saved successfully
            }
            Err(e) => {
                self.say(ctx, format!("Could not start saving: {e}"));
                false
            }
        }
    }

    /// Ctrl+E: export again with the last settings (the export card opens the first time).
    fn export_stl(&mut self, ctx: &egui::Context) {
        crate::export_dialog::quick(self, ctx);
    }

    /// Open the export card with this file type chosen.
    pub fn export_as(&mut self, ctx: &egui::Context, format: ExportFormat) {
        let defaults = crate::export_dialog::Defaults::from_preferences(&self.preferences);
        crate::export_dialog::open_with(ctx, Some(format), defaults);
    }

    /// Open the export card and measure the model for printing (size, volume, holes, thin walls, overhangs).
    pub fn print_check(&mut self, ctx: &egui::Context) {
        crate::export_dialog::open_check_with(ctx, crate::export_dialog::Defaults::from_preferences(&self.preferences));
    }

    /// Start and poll native file work without blocking a frame. Failed/cancelled results never
    /// replace the current Scene; edits made while an open was pending also prevent replacement.
    fn update_native_job(&mut self, ctx: &egui::Context) {
        if self.native_job.is_none()
            && let Some(queued) = self.queued_open.take()
        {
            if self.track_scene() != queued.base_revision {
                self.say(ctx, "The design changed while the open was waiting, so it was cancelled");
            } else {
                match crate::document_job::Job::open(ctx, queued.path.clone(), queued.base_revision) {
                    Ok(job) => self.native_job = Some(job),
                    Err(e) if e == crate::work_budget::BUSY => {
                        self.queued_open = Some(queued);
                        ctx.request_repaint_after(std::time::Duration::from_millis(80));
                    }
                    Err(e) => self.say(ctx, e),
                }
            }
        }

        let result = self.native_job.as_ref().and_then(|job| job.poll().map(|r| (job.kind, job.base_revision, job.cancel_elapsed(), r)));
        if let Some((kind, base_revision, cancel_secs, result)) = result {
            self.native_job = None;
            if let Some(secs) = cancel_secs {
                // Actual cancellation latency, not just the instant the button was clicked.
                crate::gpu_log::log(&format!("document cancellation finished after {secs:.3} s"));
            }
            match (kind, result) {
                (crate::document_job::Kind::Open, Ok(crate::document_job::Completed::Opened { scene, path, notes })) => {
                    if self.track_scene() == base_revision {
                        self.install_scene(scene, Some(path), notes);
                        self.say(ctx, "Opened");
                    } else {
                        self.say(ctx, "The design changed while opening, so the loaded file was not applied");
                    }
                }
                (crate::document_job::Kind::Save, Ok(crate::document_job::Completed::Saved { path })) => {
                    self.path = Some(path);
                    let same = self.track_scene() == base_revision;
                    self.dirty = !same;
                    if same {
                        self.doc_title.clear();
                        if let Err(e) = self.recovery.document_saved(base_revision) {
                            self.notice = Some(format!("{}{e}", i18n::tr("error.recovery_cleanup")));
                        }
                    }
                    self.say(ctx, if same { "Saved" } else { "Edited while saving: the latest changes are not saved yet" });
                    if let Some(next) = self.after_save.take() {
                        if same {
                            self.finish_pending_file_action(ctx, next);
                        } else {
                            self.pending_file_action = Some(next);
                        }
                    }
                }
                (_, Ok(_)) => self.say(ctx, "The file job returned an unexpected result"),
                (_, Err(e)) => {
                    if kind == crate::document_job::Kind::Save {
                        self.pending_file_action = self.after_save.take();
                    }
                    let text = if e == crate::export_mesh::CANCELLED {
                        "Stopped".into()
                    } else if kind == crate::document_job::Kind::Open {
                        format!("Could not open: {e}")
                    } else {
                        format!("Could not save: {e}")
                    };
                    self.say(ctx, text);
                }
            }
        }
        if self.native_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(70));
        }
    }

    fn native_job_card(&mut self, ctx: &egui::Context) {
        if self.queued_open.is_some() {
            egui::Area::new(egui::Id::new("native_file_queue")).anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -24.0]).show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.label("Waiting for another heavy job to finish before opening (memory guard)");
                    if ui.button("Cancel open").clicked() {
                        self.queued_open = None;
                    }
                });
            });
            return;
        }
        let Some(job) = self.native_job.as_mut() else { return };
        egui::Area::new(egui::Id::new("native_file_progress")).anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -24.0]).show(ctx, |ui| {
            style::card().show(ui, |ui| {
                ui.set_width(325.0);
                ui.label(
                    egui::RichText::new(match job.kind {
                        crate::document_job::Kind::Open => "Opening the design…",
                        crate::document_job::Kind::Save => "Saving the design…",
                    })
                    .strong(),
                );
                ui.label(job.phase_name());
                ui.add(egui::ProgressBar::new(job.fraction()).show_percentage());
                ui.horizontal(|ui| {
                    ui.label(format!("{:.1} s", job.elapsed()));
                    if job.can_cancel() {
                        if ui.button("Stop").clicked() {
                            let _ = job.cancel();
                        }
                    } else if job.is_committing() {
                        ui.label("This last step cannot be stopped, to protect the original file");
                    } else {
                        ui.label("Waiting for the stop to finish…");
                    }
                });
            });
        });
    }

    // ----- shortcuts -----

    pub(crate) fn keys(&mut self, ctx: &egui::Context) {
        if self.pending_file_action.is_some()
            || self.native_job.is_some()
            || self.queued_open.is_some()
            || self.recovery.pending_path().is_some()
            || crate::svg_dialog::is_open(ctx)
            || crate::array::is_open(ctx)
            || crate::mesh_dialog::is_open(ctx)
            || crate::melt_preset::is_open(ctx)
            || crate::export_dialog::is_open(ctx)
            || crate::settings_ui::is_open(ctx)
        {
            return;
        }
        // Dimensions have their own Tab/Enter/Esc lifecycle. Never dispatch text-input
        // keys as view shortcuts (in particular Tab, 1/3/5/7 or Delete).
        if self.sk.dimension.as_ref().is_some_and(|d| d.editing) {
            self.sketch_keys(ctx);
            return;
        }
        if ctx.egui_wants_keyboard_input() || palette::capturing(ctx) {
            return;
        }
        use egui::{Key, Modifiers};
        // Cancel only an active gizmo gesture (keeping Esc for everything else); do not drop the
        // selection or commit its preview.
        if self.drag.is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) && self.cancel_gizmo_drag() {
            return;
        }
        if self.sketch_keys(ctx) {
            return;
        }
        // Reserve Ctrl/Cmd+K for the command palette, even with old user bindings.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            palette::open(ctx);
            return;
        }
        if !palette::is_open(ctx) && self.custom_shortcuts(ctx) {
            return;
        }
        self.curve_keys(ctx);
        self.clipboard_events(ctx);
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let hit = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if hit(cmd_shift, Key::Z) || hit(cmd, Key::Y) {
            self.redo();
        } else if hit(cmd, Key::Z) {
            self.undo();
        }
        if hit(cmd, Key::S) {
            self.save(ctx, false);
        }
        if hit(cmd, Key::O) {
            self.open_dialog(ctx);
        }
        if hit(cmd, Key::E) {
            self.export_stl(ctx);
        }
        if hit(cmd, Key::D) {
            self.duplicate();
        }
        if hit(cmd, Key::G) {
            self.group_selected();
        }
        let plain = Modifiers::NONE;
        if hit(plain, Key::Delete) || hit(plain, Key::Backspace) {
            self.delete_selected();
        }
        if hit(plain, Key::F3) {
            self.perf.hud = !self.perf.hud;
        }
        if hit(plain, Key::Questionmark) || hit(plain, Key::F1) {
            self.show_keys = !self.show_keys;
            if self.show_keys {
                self.guide.panel_open = false;
            }
        }
        for (m, step) in [(Modifiers::NONE, 1.0), (Modifiers::SHIFT, 10.0)] {
            for (k, d) in [
                (Key::ArrowLeft, [-1.0, 0.0, 0.0]),
                (Key::ArrowRight, [1.0, 0.0, 0.0]),
                (Key::ArrowUp, [0.0, 1.0, 0.0]),
                (Key::ArrowDown, [0.0, -1.0, 0.0]),
                (Key::PageUp, [0.0, 0.0, 1.0]),
                (Key::PageDown, [0.0, 0.0, -1.0]),
            ] {
                if hit(m, k) {
                    self.nudge([d[0] * step, d[1] * step, d[2] * step]);
                }
            }
        }
        if hit(plain, Key::Tab) {
            self.flip_op();
        }
        if hit(plain, Key::F) {
            palette::open(ctx);
        }
        if hit(Modifiers::SHIFT, Key::F) {
            self.frame_view(true);
        }
        if hit(plain, Key::Home) {
            self.frame_view(false);
        }
        if hit(plain, Key::Escape) {
            self.sel = None;
        }
        // Blender-style views on the number keys
        let v = |k: Key, back: bool| hit(if back { Modifiers::CTRL } else { plain }, k);
        let pi = std::f32::consts::PI;
        for (k, yaw, pitch) in [(Key::Num1, 0.0, 0.0), (Key::Num3, pi / 2.0, 0.0), (Key::Num7, self.cam.yaw, pi / 2.0)] {
            if v(k, false) {
                self.fly_look(yaw, pitch);
            }
        }
        for (k, yaw, pitch) in [(Key::Num1, pi, 0.0), (Key::Num3, -pi / 2.0, 0.0), (Key::Num7, self.cam.yaw, -pi / 2.0)] {
            if v(k, true) {
                self.fly_look(yaw, pitch);
            }
        }
        if hit(plain, Key::Num5) {
            self.cam.ortho = !self.cam.ortho;
        }
        for (k, d) in [(Key::ArrowLeft, -1.0), (Key::ArrowRight, 1.0)] {
            if hit(Modifiers::ALT, k) {
                self.cam.yaw += d * 0.26;
            }
        }
    }

    // ----- layout -----

    fn top_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        #[cfg(test)]
        crate::settings_ui::clear_menu(&ctx);
        egui::Panel::top("top").exact_size(46.0).frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(12, 6))).show(
            ui,
            |ui| {
                let bar = ui.max_rect();
                style::panel_sheen(ui, bar.expand2(egui::vec2(12.0, 6.0)), style::PANEL_DARK, style::Edge::Bottom);
                // left: logo, file, edit, undo / redo, search.  right: export.  middle: the file name.
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    // the white mark, made once and kept in the context
                    let tex: egui::TextureHandle =
                        ui.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("logo_tex"))).unwrap_or_else(|| {
                            let h = ui.ctx().load_texture("logo", crate::icon::mark_image(), egui::TextureOptions::LINEAR);
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("logo_tex"), h.clone()));
                            h
                        });
                    // the mark and the name are one button: it opens the app menu
                    let logo = ui
                        .horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(30.0, 30.0 * 131.0 / 192.0)).tint(style::TEXT));
                            ui.label(RichText::new("Melt 3D").size(14.0).strong().color(style::TEXT));
                        })
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(i18n::tr("menu.menu"));
                    #[cfg(test)]
                    {
                        crate::settings_ui::record_menu(&ctx, "anchor", logo.rect);
                        crate::settings_ui::record_menu_anchor(&ctx, logo.id);
                    }
                    egui::Popup::menu(&logo).show(|ui| {
                        if ui.button("New").clicked() {
                            self.new_scene(Scene::default());
                            ui.close();
                        }
                        if ui.button(style::keys("Open…  Ctrl+O")).clicked() {
                            self.open_dialog(&ctx);
                            ui.close();
                        }
                        if ui.button("Puffy Text…").on_hover_text("Turn Japanese or English letters into puffy solids").clicked() {
                            crate::puffy_text_ui::open(self, &ctx);
                            ui.close();
                        }
                        if ui.button("Chess Set…").on_hover_text("Chess pieces in three designs").clicked() {
                            crate::chess_dialog::open(&ctx, self);
                            ui.close();
                        }
                        if ui.button(style::keys("Save  Ctrl+S")).clicked() {
                            self.save(&ctx, false);
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.save(&ctx, true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Export…").clicked() {
                            crate::export_dialog::open_with(&ctx, None, crate::export_dialog::Defaults::from_preferences(&self.preferences));
                            ui.close();
                        }
                        ui.separator();
                        let about = ui.button("About Melt 3D");
                        #[cfg(test)]
                        crate::settings_ui::record_menu(&ctx, "about", about.rect);
                        if about.clicked() {
                            self.say(&ctx, format!("Melt 3D {}", env!("CARGO_PKG_VERSION")));
                            ui.close();
                        }
                        let settings = ui.button(crate::i18n::tr("settings.open"));
                        #[cfg(test)]
                        crate::settings_ui::record_menu(&ctx, "settings", settings.rect);
                        if settings.clicked() {
                            crate::settings_ui::open(&ctx, &self.preferences);
                            ui.close();
                        }
                    });
                    style::vsep(ui);
                    style::menu_button(ui, i18n::tr("menu.file"), false, |ui| {
                        if ui.button("New").clicked() {
                            self.new_scene(Scene::default());
                            ui.close();
                        }
                        if ui.button(style::keys("Open…  Ctrl+O")).clicked() {
                            self.open_dialog(&ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.scene_structure_locked(), egui::Button::new("Add file…"))
                            .on_hover_text("Put another Melt 3D file into this one. It melts with what is already here")
                            .clicked()
                        {
                            self.add_file_dialog(&ctx);
                            ui.close();
                        }
                        if ui.add_enabled(!self.scene_structure_locked(), egui::Button::new("Import SVG…")).clicked() {
                            crate::svg_dialog::open(&ctx);
                            ui.close();
                        }
                        if ui
                            .add_enabled(!self.scene_structure_locked(), egui::Button::new("Import mesh…"))
                            .on_hover_text("STL, OBJ, 3MF, PLY, GLB")
                            .clicked()
                        {
                            crate::mesh_dialog::open(&ctx, self.document_generation());
                            ui.close();
                        }
                        if ui.button(style::keys("Save  Ctrl+S")).clicked() {
                            self.save(&ctx, false);
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.save(&ctx, true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Export…").on_hover_text("STL, 3MF, OBJ, PLY, GLB").clicked() {
                            crate::export_dialog::open_with(&ctx, None, crate::export_dialog::Defaults::from_preferences(&self.preferences));
                            ui.close();
                        }
                        if ui.button(style::keys("Export again with last settings  Ctrl+E")).clicked() {
                            self.export_stl(&ctx);
                            ui.close();
                        }
                        if ui.button("Export image (PNG)").clicked() {
                            self.start_png(&ctx);
                            ui.close();
                        }
                    });
                    style::menu_button(ui, i18n::tr("menu.edit"), false, |ui| {
                        if ui.button(style::keys("Copy  Ctrl+C")).clicked() {
                            self.copy(&ctx);
                            ui.close();
                        }
                        ui.add_enabled_ui(!self.scene_structure_locked(), |ui| {
                            if ui.button(style::keys("Cut  Ctrl+X")).clicked() {
                                self.cut(&ctx);
                                ui.close();
                            }
                            if ui.button(style::keys("Paste  Ctrl+V")).clicked() {
                                self.paste(&ctx, None);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(style::keys("Duplicate  Ctrl+D")).clicked() {
                                self.duplicate();
                                ui.close();
                            }
                            if ui.button("Array copy…").clicked() {
                                crate::array::open(&ctx);
                                ui.close();
                            }
                            ui.menu_button("Pattern", |ui| {
                                for k in crate::pattern::PatternKind::ALL {
                                    if ui.button(k.label()).on_hover_text(k.hint()).clicked() {
                                        self.set_pattern_kind(&ctx, k);
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("Clear pattern").clicked() {
                                    self.clear_pattern();
                                    ui.close();
                                }
                            });
                            if ui.button("Make instance").on_hover_text("Linked copy: edits to the original update it too").clicked() {
                                self.make_instance(&ctx);
                                ui.close();
                            }
                            if ui.button("Mirror copy").clicked() {
                                self.mirror_copy();
                                ui.close();
                            }
                            if ui.button("Delete  Del").clicked() {
                                self.delete_selected();
                                ui.close();
                            }
                        });
                        ui.separator();
                        if ui.button("Select none  Esc").clicked() {
                            self.sel = None;
                            self.multi.clear();
                            ui.close();
                        }
                        if ui.button("Show all hidden").clicked() {
                            for g in &mut self.scene.groups {
                                g.visible = true;
                                for sh in &mut g.shapes {
                                    sh.visible = true;
                                }
                            }
                            ui.close();
                        }
                    });
                    style::menu_button(ui, i18n::tr("menu.view"), false, |ui| {
                        let pi = std::f32::consts::PI;
                        for (name, yaw, pitch) in [
                            ("Front  1", 0.0, 0.0),
                            ("Right  3", pi / 2.0, 0.0),
                            ("Top  7", self.cam.yaw, pi / 2.0),
                            ("Back  Ctrl+1", pi, 0.0),
                            ("Left  Ctrl+3", -pi / 2.0, 0.0),
                            ("Bottom  Ctrl+7", self.cam.yaw, -pi / 2.0),
                        ] {
                            if ui.button(style::keys(name)).clicked() {
                                self.fly_look(yaw, pitch);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Turn left  Alt+←").clicked() {
                            self.cam.yaw -= 0.26;
                        }
                        if ui.button("Turn right  Alt+→").clicked() {
                            self.cam.yaw += 0.26;
                        }
                        ui.separator();
                        if ui.button(if self.cam.ortho { "Perspective view  5" } else { "Flat (ortho) view  5" }).clicked() {
                            self.cam.ortho = !self.cam.ortho;
                            ui.close();
                        }
                        ui.horizontal(|ui| {
                            for m in crate::view_mode::ViewMode::ALL {
                                if crate::view_mode::swatch(ui, m, 28.0, self.view_mode == m).clicked() {
                                    self.view_mode = m;
                                }
                            }
                        });
                        if ui.button("Frame all  Home").clicked() {
                            self.frame_view(false);
                            ui.close();
                        }
                        if ui.button("Frame selection  Shift+F").clicked() {
                            self.frame_view(true);
                            ui.close();
                        }
                    });
                    style::vsep(ui);
                    if ui
                        .add_enabled(!self.scene_structure_locked() || self.sk.live.is_some(), |ui: &mut Ui| {
                            style::icon_button(ui, 32.0, &style::keys("Undo (Ctrl+Z)"), false, glyphs::undo)
                        })
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(!self.scene_structure_locked(), |ui: &mut Ui| {
                            style::icon_button(ui, 32.0, &style::keys("Redo (Ctrl+Shift+Z)"), false, glyphs::redo)
                        })
                        .clicked()
                    {
                        self.redo();
                    }
                    style::vsep(ui);
                    if style::pill(ui, &format!("{}  F", i18n::tr("menu.find")), false).on_hover_text(i18n::tr("menu.find.tip")).clicked() {
                        palette::open(&ctx);
                    }
                    let left_end = ui.cursor().left();
                    let right_start = ui.max_rect().right();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if style::icon_button(ui, 32.0, i18n::tr("tool.solidcraft.tip"), false, |p, r, _| style::solidcraft_logo(p, r)).clicked() {
                            self.open_precise(&ctx);
                        }
                        style::menu_button(ui, i18n::tr("menu.export"), true, |ui| {
                            if ui.button("Export…").on_hover_text("STL, 3MF, OBJ, PLY, GLB. Detail, units, color").clicked() {
                                crate::export_dialog::open_with(&ctx, None, crate::export_dialog::Defaults::from_preferences(&self.preferences));
                                ui.close();
                            }
                            if ui.button("Export again").on_hover_text(style::keys("Ctrl+E")).clicked() {
                                self.export_stl(&ctx);
                                ui.close();
                            }
                            if ui.button("Image (PNG)").on_hover_text("Save as seen on screen").clicked() {
                                self.start_png(&ctx);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Print check").on_hover_text("Size, volume, holes").clicked() {
                                self.print_check(&ctx);
                                ui.close();
                            }
                        });
                        // Mouse or trackpad: Auto follows what the last scroll looked like
                        {
                            let eff = self.input_dev.device(self.input_seen);
                            let items = [
                                ("Mouse", "Wheel zooms, middle-drag turns, right-drag slides"),
                                ("Trackpad", "Two fingers turn (Shift: slide), pinch zooms"),
                            ];
                            // nothing is chosen yet: the highlighted one is what the scrolling looked like
                            let cur = match eff {
                                crate::view::Device::Mouse => 0,
                                crate::view::Device::Trackpad => 1,
                            };
                            let cur = match self.input_dev {
                                crate::view::InputDev::Mouse => 0,
                                crate::view::InputDev::Trackpad => 1,
                                crate::view::InputDev::Auto => cur,
                            };
                            if let Some(i) = style::segmented(ui, "input_dev", &items, Some(cur), false) {
                                self.input_dev = if i == 0 { crate::view::InputDev::Mouse } else { crate::view::InputDev::Trackpad };
                            }
                        }
                        let mut menu = crate::perf::MenuState { quality: self.perf.quality, hud: self.perf.hud, bench: false };
                        crate::perf::quality_menu(ui, &mut menu, |ui| self.gpu_menu_items(ui));
                        self.perf.quality = menu.quality;
                        self.perf.hud = menu.hud;
                        self.perf.bench_requested |= menu.bench;
                    });
                    // the design's name in the middle of the bar: click it to type a new one
                    let cx = bar.center().x.clamp(left_end + 60.0, (right_start - 220.0).max(left_end + 60.0));
                    let mut commit = false;
                    let mut cancel = false;
                    if let Some(text) = self.title_edit.as_mut() {
                        let r = egui::Rect::from_center_size(egui::pos2(cx, bar.center().y), egui::vec2(220.0, 24.0));
                        let edit = ui.put(
                            r,
                            egui::TextEdit::singleline(text).char_limit(MAX_TITLE_CHARS).horizontal_align(egui::Align::Center).desired_width(220.0),
                        );
                        if !edit.has_focus() && !edit.lost_focus() {
                            edit.request_focus();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            cancel = true;
                        } else if edit.lost_focus() {
                            commit = true;
                        }
                    } else {
                        let name =
                            if self.path.is_none() && self.doc_title.is_empty() { i18n::tr("status.untitled").to_string() } else { self.doc_name() };
                        let label = format!("{}{}", name, if self.dirty { " *" } else { "" });
                        let shown = ui.painter().text(
                            egui::pos2(cx, bar.center().y),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::proportional(13.0),
                            style::DIM,
                        );
                        let hit = ui.interact(shown.expand2(egui::vec2(8.0, 6.0)), egui::Id::new("doc_name"), egui::Sense::click());
                        let hit = hit.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Click to rename this design");
                        if hit.clicked() {
                            self.title_edit = Some(self.doc_name());
                        }
                    }
                    if cancel {
                        self.title_edit = None;
                    } else if commit {
                        let typed = self.title_edit.take().unwrap_or_default();
                        let ctx = ui.ctx().clone();
                        self.rename_doc(&ctx, &typed);
                    }
                });
            },
        );
    }

    /// The tools down the left side of the viewport: the shapes to add, the curve, the operation, groups.
    fn tool_strip(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let rect = self.view_rect;
        let strip = egui::Area::new(egui::Id::new("tool_strip")).order(egui::Order::Middle).fixed_pos(rect.left_top() + egui::vec2(12.0, 12.0)).show(
            &ctx,
            |ui| {
                style::strip().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
                    // a strip that would not fit grows a second column so everything shows at once;
                    // only when even that is too tall does it scroll
                    let max_h = (rect.height().min(ctx.content_rect().bottom() - rect.top()) - 30.0).max(120.0);
                    let narrow_h = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("strip_narrow_h"))).unwrap_or(0.0);
                    let wide = narrow_h > max_h;
                    let strip_w = if wide { style::STRIP_W * 2.0 + 4.0 } else { style::STRIP_W };
                    // The auto-sized Area otherwise keeps its previous (initially 400px)
                    // height as the scroll limit, even when the viewport has room to grow.
                    ui.set_max_height(max_h);
                    #[cfg(test)]
                    let mut strip_tail = None;
                    let out = egui::ScrollArea::vertical().max_height(max_h).auto_shrink([true, true]).show(ui, |ui| {
                        ui.set_max_width(strip_w);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            self.solid_mode_button(ui);
                            if self.solid_mode {
                                self.sketch_button(ui);
                                self.sketch_make_buttons(ui);
                            } else {
                                for k in Kind::MAIN {
                                    let r = ui.add_enabled(!self.scene_structure_locked(), |ui: &mut Ui| {
                                        style::wide_button(ui, i18n::kind(k), i18n::kind(k), false, None, |p, r, ink| {
                                            glyphs::kind(p, r, k, ink, ink.gamma_multiply(0.15))
                                        })
                                    });
                                    if r.clicked() {
                                        self.add_shape(&ctx, k);
                                    }
                                }
                                self.curve_button(ui);
                                self.pipe_button(ui);
                            }
                            if !self.solid_mode {
                                style::menu_button_enabled(ui, "…", false, !self.scene_structure_locked(), |ui| {
                                    ui.set_min_width(190.0);
                                    ui.label(egui::RichText::new(i18n::tr("tool.more.title")).size(12.0).color(style::DIM));
                                    for k in Kind::MORE {
                                        let r = ui.horizontal(|ui| {
                                            let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                            let ink = ui.visuals().text_color();
                                            glyphs::kind(ui.painter(), rc, k, ink, ink.gamma_multiply(0.15));
                                            ui.selectable_label(false, i18n::kind(k))
                                        });
                                        if r.inner.clicked() {
                                            self.add_shape(&ctx, k);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(i18n::tr("tool.melt_preset")).on_hover_text(i18n::tr("tool.melt_preset.tip")).clicked() {
                                        crate::melt_preset::open(&ctx);
                                        ui.close();
                                    }
                                    if ui.button(i18n::tr("tool.organic_blob")).on_hover_text(i18n::tr("tool.organic_blob.tip")).clicked() {
                                        crate::organic_blob::open(&ctx);
                                        ui.close();
                                    }
                                    ui.separator();
                                    ui.label(egui::RichText::new(i18n::tr("tool.organic_tube")).size(12.0).color(style::DIM));
                                    for preset in crate::organic_tube::Preset::ALL {
                                        if ui.button(preset.label()).on_hover_text(i18n::tr("tool.organic_tube.tip")).clicked() {
                                            self.add_organic_tube(&ctx, preset);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    ui.label(egui::RichText::new(i18n::tr("tool.organic_furniture")).size(12.0).color(style::DIM));
                                    for design in crate::organic_furniture::Design::ALL {
                                        if ui.button(design.label()).on_hover_text(i18n::tr("tool.organic_furniture.tip")).clicked() {
                                            crate::organic_furniture::insert(self, &ctx, design);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    ui.label(egui::RichText::new(i18n::tr("tool.cad")).size(12.0).color(style::DIM));
                                    for k in Kind::CAD {
                                        let r = ui.horizontal(|ui| {
                                            let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                            let ink = ui.visuals().text_color();
                                            glyphs::kind(ui.painter(), rc, k, ink, ink.gamma_multiply(0.15));
                                            ui.selectable_label(false, i18n::kind(k))
                                        });
                                        if r.inner.clicked() {
                                            self.add_shape(&ctx, k);
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            ui.end_row();
                            style::hsep_w(ui, strip_w);
                            ui.end_row();
                            let ops: &[Op] =
                                if self.solid_mode { &[Op::Add, Op::Subtract] } else { &[Op::Add, Op::Subtract, Op::Paint, Op::Intersect] };
                            for &op in ops {
                                let on = self.new_op == op;
                                let (name, tip) = match op {
                                    Op::Add => (i18n::tr("tool.op.add"), i18n::tr("tool.op.add.tip")),
                                    Op::Subtract => (i18n::tr("tool.op.cut"), i18n::tr("tool.op.cut.tip")),
                                    Op::Paint => (i18n::tr("tool.op.paint"), i18n::tr("tool.op.paint.tip")),
                                    Op::Intersect => (i18n::tr("tool.op.overlap"), i18n::tr("tool.op.overlap.tip")),
                                };
                                if style::wide_button(ui, name, tip, on, Some(style::op_color(op)), |p, r, ink| glyphs::op(p, r, op, ink)).clicked() {
                                    self.new_op = op;
                                    self.live_update();
                                }
                            }
                            ui.end_row();
                            // solid mode keeps only what makes a solid: the rest hides until you leave it
                            if self.solid_mode {
                                return;
                            }
                            style::hsep_w(ui, strip_w);
                            ui.end_row();
                            if ui
                                .add_enabled(!self.scene_structure_locked(), |ui: &mut Ui| {
                                    style::wide_button(ui, i18n::tr("tool.group.new"), i18n::tr("tool.group.new.tip"), false, None, glyphs::group)
                                })
                                .clicked()
                            {
                                self.add_group();
                            }
                            let melting = self.selection_melts();
                            if style::wide_button(ui, i18n::tr("tool.melt"), i18n::tr("tool.melt.tip"), melting, None, crate::glyphs_more::melt)
                                .clicked()
                            {
                                self.toggle_melt_selected();
                            }
                            let pr = ui.add_enabled(!self.scene_structure_locked(), |ui: &mut Ui| {
                                style::wide_button(ui, i18n::tr("tool.pattern"), i18n::tr("tool.pattern.tip"), false, None, |p, r, ink| {
                                    crate::pattern_ui::icon(crate::pattern::PatternKind::Circular, p, r, ink)
                                })
                            });
                            #[cfg(test)]
                            {
                                strip_tail = Some((pr.rect, ui.clip_rect()));
                            }
                            egui::Popup::menu(&pr).show(|ui| {
                                ui.set_min_width(170.0);
                                for k in crate::pattern::PatternKind::ALL {
                                    let r = ui.horizontal(|ui| {
                                        let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                        crate::pattern_ui::icon(k, ui.painter(), rc, ui.visuals().text_color());
                                        ui.selectable_label(false, k.label())
                                    });
                                    if r.inner.clicked() {
                                        self.set_pattern_kind(&ctx, k);
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.selectable_label(false, "Linked copy").clicked() {
                                    self.make_instance(&ctx);
                                    ui.close();
                                }
                                if ui.selectable_label(false, "Remove pattern").clicked() {
                                    self.clear_pattern();
                                    ui.close();
                                }
                            });
                        });
                    });
                    #[cfg(test)]
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("tool_strip_tail_layout"), (strip_tail, out.content_size.y, max_h)));
                    if !wide {
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("strip_narrow_h"), out.content_size.y));
                    }
                });
            },
        );
        // Floating command cards use the actual occupied area, including a second
        // column or scrollbar, so they can stay beside the tools after a resize.
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("tool_strip_rect"), strip.response.rect));
    }

    /// The docked panel on the left: the scene list (outliner) and the ready-made models (assets).
    fn left_dock(&mut self, ui: &mut Ui) {
        // drag its right edge to make it wider or narrower
        egui::Panel::left("dock")
            .default_size(272.0)
            .size_range(220.0..=560.0)
            .resizable(true)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 10)))
            .show(ui, |ui| {
                style::panel_sheen(ui, ui.max_rect().expand2(egui::vec2(10.0, 10.0)), style::PANEL_DARK, style::Edge::Left);
                let tabs = [(i18n::tr("scene.tab.scene"), ""), (i18n::tr("scene.tab.library"), "")];
                if let Some(i) = style::segmented(ui, "dock_tabs", &tabs, Some(usize::from(self.lib_open)), true) {
                    let was_open = self.lib_open;
                    self.lib_open = i == 1;
                    if was_open && !self.lib_open {
                        crate::library_ui::suspend(ui.ctx());
                    }
                }
                ui.add_space(6.0);
                if self.lib_open {
                    ui.add_enabled_ui(!self.scene_structure_locked(), |ui| crate::library_ui::panel(self, ui));
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} {}", self.scene.shape_count(), i18n::tr("scene.shapes"))).size(12.0).color(style::DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let several = self.picks().len() > 1;
                            if style::pill_enabled(
                                ui,
                                if several { i18n::tr("scene.group_them") } else { i18n::tr("scene.group") },
                                false,
                                !self.scene_structure_locked(),
                            )
                            .on_hover_text(style::keys(i18n::tr("scene.group_tip")))
                            .clicked()
                            {
                                if several {
                                    self.group_selected();
                                } else {
                                    self.add_group();
                                }
                            }
                        });
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        self.layers(ui);
                    });
                }
            });
    }

    /// A card listing the mouse and keys.
    fn keys_card(&mut self, ui: &mut Ui) {
        if !self.show_keys {
            return;
        }
        let ctx = ui.ctx().clone();
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("keys_card"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(rect.right_bottom() - egui::vec2(16.0, 16.0))
            .show(&ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.label(RichText::new("Controls").strong().size(14.0));
                    // the first-use box practice and the hidden status hints come back from here
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(i18n::tr("guide.open")).on_hover_text(i18n::tr("guide.open.tip")).clicked() {
                            self.guide.open();
                            self.show_keys = false;
                        }
                        if !self.guide.prefs.hidden.is_empty() && ui.button(i18n::tr("guide.reset_hints")).clicked() {
                            self.guide.reset_hints();
                        }
                    });
                    ui.separator();
                    for (k, v) in [
                        ("Middle drag", "Orbit"),
                        ("Right drag", "Pan"),
                        ("Wheel", "Zoom to cursor"),
                        ("Left drag (empty)", "Orbit"),
                        ("Click", "Select  Ctrl: add/remove  Shift: add/remove"),
                        ("Double click", "Focus on shape"),
                        ("Drawing: left click → right click", "Place point → finish (all draw tools)"),
                        ("F", "Find shapes, library, commands"),
                        ("Shift+F / Home", "Frame selection / Frame all"),
                        ("1 3 7 (Ctrl: opposite)", "Front / Right / Top"),
                        ("5", "Perspective ⇄ Ortho"),
                        ("Tab", "Add ⇄ Subtract"),
                        ("Ctrl + drag", "1 mm / 15° steps"),
                        ("Arrow keys", "Move 1 mm (Shift: 10 mm)"),
                        ("Ctrl+C / X / V", "Copy / Cut / Paste"),
                        ("Ctrl+D / Delete", "Duplicate / Delete"),
                        ("Ctrl+G", "Group the selected shapes"),
                        ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(style::keys(k)).size(12.0).color(style::ACCENT)));
                            ui.label(RichText::new(v).size(12.0).color(style::TEXT));
                        });
                    }
                });
            });
    }

    /// A single quiet status strip instead of a stack of interrupting tooltips/popups.
    fn status_bar(&mut self, ui: &mut Ui) {
        let now = ui.input(|i| i.time);
        let info = crate::guide::hint(&self.sk, self.tool, self.sel.is_some(), self.scene.shape_count(), self.new_op, self.ct.drawing, self.pt.on);
        egui::Panel::bottom("status")
            .exact_size(32.0)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(12, 5)))
            .show(ui, |ui| {
                style::panel_sheen(ui, ui.max_rect().expand2(egui::vec2(12.0, 5.0)), style::PANEL_DARK, style::Edge::Top);
                ui.horizontal_centered(|ui| {
                    let note = if now - self.status.1 < 5.0 { self.status.0.as_str() } else { "" };
                    if !note.is_empty() {
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((5.0 - (now - self.status.1)).max(0.0) + 0.05));
                    }
                    let error = self.sketch_active() && !self.sk.msg.is_empty();
                    let hidden = info.id.is_some_and(|id| self.guide.prefs.is_hidden(id));
                    let message = if error {
                        format!("{}: {}  {}", i18n::tr("guide.status.fix"), self.sk.msg, i18n::tr("guide.status.fix.where"))
                    } else if !note.is_empty() {
                        note.to_string()
                    } else if hidden {
                        // this kind of hint was hidden with ×: the plain tool hint stays
                        crate::gizmo::hint(self.tool).to_string()
                    } else {
                        format!("{}: {}  |  {}", i18n::tr("guide.status.next"), info.action, info.controls)
                    };
                    let severity = error || (note.is_empty() && info.warning);
                    let ink = if severity { style::CUT } else { style::DIM };
                    let text = egui::RichText::new(message).size(12.0).color(ink);
                    let room = (ui.available_width() - if ui.available_width() < 740.0 { 170.0 } else { 330.0 }).max(100.0);
                    ui.add_sized([room, 18.0], egui::Label::new(text).truncate()).on_hover_text(format!("{}  |  {}", info.action, info.controls));
                    if !error
                        && note.is_empty()
                        && !info.warning
                        && !hidden
                        && let Some(id) = info.id
                        && ui.small_button("×").on_hover_text(i18n::tr("guide.status.dismiss.tip")).clicked()
                    {
                        self.guide.dismiss(id);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.selectable_label(self.an.open, i18n::tr("status.animation")).on_hover_text(i18n::tr("status.animation.tip")).clicked() {
                            self.an.open = !self.an.open;
                        }
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| i18n::tr("status.untitled").into());
                        let dims = self.selection_size().map(|d| format!(" | {d}")).unwrap_or_default();
                        ui.label(
                            RichText::new(format!(
                                "{}{} | {} {}{}",
                                name,
                                if self.dirty { " *" } else { "" },
                                i18n::tr("status.shapes"),
                                self.scene.shape_count(),
                                dims
                            ))
                            .size(12.0)
                            .color(style::DIM),
                        );
                    });
                });
            });
    }

    /// The size of the selected shape for the status bar ("幅 30 × 奥行 30 × 高さ 30 mm", or a rough
    /// diameter when it is turned or is a group).
    fn selection_size(&self) -> Option<String> {
        let units = self.preferences.units();
        let decimals = self.preferences.decimals();
        match self.sel? {
            Pick::Shape(g, s) => {
                let sh = self.scene.shape(g, s)?;
                let h = sh.local_half();
                let turned = sh.rot.iter().any(|r| (r % 90.0).abs() > 0.01);
                if turned {
                    return Some(format!("{} {}", i18n::tr("status.about"), units.length(crate::view::radius_of(sh) * 2.0, decimals)));
                }
                // turned by whole quarter turns: the sizes along the world's axes, swapped as turned (#195)
                let r = crate::sdf::rotation(sh.rot);
                let dim = |i: usize| (0..3).map(|j| r[i][j].abs() * h[j] * sh.scale[j]).sum::<f32>() * 2.0;
                Some(format!(
                    "{} {} × {} {} × {} {} {}",
                    i18n::tr("status.width"),
                    units.number(dim(0), decimals),
                    i18n::tr("status.depth"),
                    units.number(dim(1), decimals),
                    i18n::tr("status.height"),
                    units.number(dim(2), decimals),
                    units.label()
                ))
            }
            Pick::Group(g) => {
                let (_, r) = crate::gpu::bounds(&self.scene, Some(Pick::Group(g)));
                (r > 0.0).then(|| format!("{} {} {}", i18n::tr("status.group"), i18n::tr("status.about"), units.length(r * 2.0, decimals)))
            }
        }
    }

    // ----- picture export -----

    /// Ask where to save, then take a clean picture of the viewport a moment later.
    fn start_png(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("Untitled.png").save_file() else { return };
        self.capture = Some(Capture { path, frames: 0 });
        ctx.request_repaint();
    }

    /// Drives the picture: hide the overlays for two frames, ask for a screenshot, crop and save it.
    fn capture_step(&mut self, ctx: &egui::Context) {
        let Some(cap) = self.capture.as_mut() else { return };
        cap.frames += 1;
        ctx.request_repaint();
        if cap.frames == 3 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(img) = image else {
            if cap.frames > 120 {
                self.capture = None;
            }
            return;
        };
        let Some(cap) = self.capture.take() else { return };
        let ppp = ctx.pixels_per_point();
        let r = self.view_rect;
        let (w, h) = (img.width() as i64, img.height() as i64);
        let x0 = ((r.min.x * ppp).round() as i64).clamp(0, w);
        let y0 = ((r.min.y * ppp).round() as i64).clamp(0, h);
        let x1 = ((r.max.x * ppp).round() as i64).clamp(x0, w);
        let y1 = ((r.max.y * ppp).round() as i64).clamp(y0, h);
        let mut rgba = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4).max(0) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(c) = img.pixels.get((y * w + x) as usize) {
                    rgba.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                }
            }
        }
        let res =
            crate::png::encode_rgba((x1 - x0) as u32, (y1 - y0) as u32, &rgba).and_then(|b| std::fs::write(&cap.path, b).map_err(|e| e.to_string()));
        match res {
            Ok(()) => self.say(ctx, "Image saved"),
            Err(e) => self.say(ctx, format!("Could not save image: {e}")),
        }
    }

    /// For testing without a person: with `MELTCAD_TEST_DIR=dir`, a file `dir/snap` asks for a
    /// screenshot (`dir/shot.ppm`) and `dir/quit` closes the window.
    fn test_hooks(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.shot.clone() else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        if dir.join("png").exists() && std::fs::remove_file(dir.join("png")).is_ok() {
            self.capture = Some(Capture { path: dir.join("out.png"), frames: 0 });
        }
        // `hud` shows or hides the frame-time overlay, `bench` starts the speed test window
        if dir.join("hud").exists() && std::fs::remove_file(dir.join("hud")).is_ok() {
            self.perf.hud = !self.perf.hud;
        }
        if dir.join("where").exists() {
            // where the overlay is (written to `dir/where.txt` every frame while the file `where` exists)
            let r = ctx.memory(|m| m.area_rect(egui::Id::new("frame_hud")));
            let _ = std::fs::write(
                dir.join("where.txt"),
                format!("hud {} area {r:?} view {:?} ppp {}\n", self.perf.hud, self.view_rect, ctx.pixels_per_point()),
            );
        }
        if dir.join("bench").exists() && std::fs::remove_file(dir.join("bench")).is_ok() {
            self.perf.bench_requested = true;
        }
        // `lib` opens the library card; `insert` puts the first people-model and a few more into the scene
        if dir.join("lib").exists() && std::fs::remove_file(dir.join("lib")).is_ok() {
            self.lib_open = true;
        }
        if dir.join("insert").exists() && std::fs::remove_file(dir.join("insert")).is_ok() {
            for (c, i) in [(0, 0), (2, 1), (4, 6)] {
                if let Some(item) = crate::library::all().get(c).and_then(|c| c.items.get(i)) {
                    crate::library_ui::insert(self, ctx, item);
                }
            }
            self.lib_open = false;
        }
        if dir.join("quit").exists() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if dir.join("snap").exists() && std::fs::remove_file(dir.join("snap")).is_ok() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let mut out = format!("P6\n{} {}\n255\n", img.width(), img.height()).into_bytes();
            for c in &img.pixels {
                out.extend_from_slice(&[c.r(), c.g(), c.b()]);
            }
            let _ = std::fs::write(dir.join("shot.ppm.tmp"), out).and_then(|()| std::fs::rename(dir.join("shot.ppm.tmp"), dir.join("shot.ppm")));
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.preferences.save_storage(storage);
        if let Some(p) = self.precise.as_ref() {
            self.precise_prefs = Some(p.app.prefs());
        }
        if let Some(prefs) = self.precise_prefs.as_ref() {
            storage.set_string(crate::precise::PREFS_KEY, prefs.clone());
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.precise_on
            && let Some(p) = self.precise.as_mut()
        {
            p.app.raw_input_hook(input);
        }
        let _ = ctx;
    }
    fn on_exit(&mut self) {
        // A control picture that has not been renamed yet must not be published.
        self.render_job = None;
        if let Some(p) = self.precise.as_mut() {
            solidcraft_ui_egui::agent_chat::shutdown(&mut p.app);
            solidcraft_ui_egui::save_job::tick(&mut p.app, true);
            solidcraft_ui_egui::recovery_ui::close(&mut p.app);
        }
        self.finish_recovery_on_exit();
    }
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame_body(ui);
    }
}

impl App {
    fn checkpoint_unsaved_for_device_loss(&mut self) {
        if !self.has_unsaved_work() {
            return;
        }
        let rev = self.track_scene();
        let now = std::time::Instant::now();
        if let Err(e) =
            self.recovery.checkpoint(&self.scene, self.path.as_deref(), true, rev, now + crate::recovery::INTERVAL.max(self.preferences.autosave()))
        {
            crate::gpu_log::log(&format!("could not save recovery data after graphics device loss: {e}"));
        }
    }

    fn finish_recovery_on_exit(&mut self) {
        if !self.gpu_device_lost
            && let Err(e) = self.recovery.normal_exit()
        {
            crate::gpu_log::log(&format!("recovery cleanup failed: {e}"));
        }
    }

    /// Which document the scene shows: a transfer started for another one must not land here.
    pub(crate) fn document_generation(&self) -> u64 {
        self.document_generation
    }

    pub(crate) fn scene_edit_in_progress(&self) -> bool {
        self.scene_structure_locked()
            || self.sketch_active()
            || self.drag.is_some()
            || self.ct.drag.is_some()
            || self.ct.drawing
            || self.ct.draw_shape.is_some()
            || self.pt.tracing
            || self.layer_drag.is_some()
            || self.marquee_from.is_some()
            || self.renaming.is_some()
            || self.title_edit.is_some()
            || self.pending_file_action.is_some()
    }

    /// Runs the commands that arrived on the control channel (at most a few per frame).
    pub(crate) fn run_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control.take() else { return };
        let mut ran = false;
        for _ in 0..32 {
            let Ok(req) = rx.try_recv() else { break };
            // commands about the window itself are handled here; the rest change the scene
            if req.cmd.get("cmd").and_then(serde_json::Value::as_str) == Some("ui.precise") {
                let result = match req.cmd.get("on") {
                    None => Ok(true),
                    Some(serde_json::Value::Bool(on)) => Ok(*on),
                    Some(_) => Err(i18n::tr("error.bool_param").to_owned()),
                }
                .and_then(|on| {
                    if ctx.egui_wants_keyboard_input() || ctx.input(|i| i.pointer.any_down()) {
                        return Err(i18n::tr("error.scene_locked").into());
                    }
                    self.set_precise_mode(ctx, on)?;
                    Ok(serde_json::json!({"precise": self.precise_on, "importing": self.precise_transfer_pending()}))
                });
                let _ = req.reply.send(result);
                ran = true;
                continue;
            }
            let name = req.cmd.get("cmd").and_then(serde_json::Value::as_str);
            if matches!(name, Some("render" | "anim.render")) {
                // Drawing four views or a whole animation can take longer than a
                // frame: do it on a worker and answer when the complete file exists.
                let crate::control::Request { cmd, reply } = req;
                if let Err(e) = self.start_control_picture(ctx, &cmd, reply) {
                    self.say(ctx, e);
                }
                ran = true;
                continue;
            }
            let read_only = matches!(name, Some("commands" | "scene.get" | "scene.summary" | "render" | "anim.get" | "anim.render"));
            let busy = self.precise_on || self.scene_edit_in_progress() || ctx.egui_wants_keyboard_input() || ctx.input(|i| i.pointer.any_down());
            let result = if !read_only && busy {
                Err("Finish or cancel the active edit before changing or saving the scene".into())
            } else {
                // Commands may sanitize even a read, or partially change a scene before
                // returning an error. Only successful mutations reach the live document.
                let mut next = self.scene.clone();
                let result = crate::automation::execute(&mut next, &req.cmd);
                if let Ok(value) = &result
                    && !read_only
                    && name != Some("save")
                    && next != self.scene
                {
                    // Keep a completed UI edit separate from each queued command's undo
                    // step, even when several commands arrive in the same frame.
                    self.settle(ctx);
                    self.clear_document_gestures();
                    self.scene = next;
                    // A command that replaces the whole design starts a new document, like
                    // File > New / Open does: no transfer from the old one may land in it.
                    if matches!(name, Some("scene.new" | "scene.set" | "load")) {
                        self.document_generation = self.document_generation.wrapping_add(1);
                    }
                    self.sel = if let Some(g) = value.get("group").and_then(serde_json::Value::as_u64) {
                        Some(match value.get("shape").and_then(serde_json::Value::as_u64) {
                            Some(s) => Pick::Shape(g as usize, s as usize),
                            None => Pick::Group(g as usize),
                        })
                    } else if matches!(name, Some("scene.new" | "scene.set" | "load")) {
                        self.scene.shape(0, 0).map(|_| Pick::Shape(0, 0))
                    } else if matches!(name, Some("shape.delete" | "group.delete")) {
                        None
                    } else {
                        self.sel
                    };
                    self.fix_selection();
                    self.settle(ctx);
                }
                if let Ok(value) = &result
                    && !read_only
                    && name != Some("save")
                {
                    // Control commands have the same explicit effect as CLI commands:
                    // playback and UI auto-capture must not replace or recapture them.
                    let frame = if name == Some("anim.at") {
                        value.get("frame").and_then(serde_json::Value::as_u64).and_then(|f| u32::try_from(f).ok()).unwrap_or(self.an.frame)
                    } else {
                        self.an.frame
                    };
                    self.animation_at_applied(frame);
                }
                result
            };
            let _ = req.reply.send(result);
            ran = true;
        }
        self.control = Some(rx);
        if ran {
            ctx.request_repaint();
        }
    }

    /// A `render` / `anim.render` request from `--control`: copy the scene here (cheap next
    /// to drawing) and let a worker draw and write the file. The answer goes out when the
    /// complete picture is on disk, so the window keeps drawing frames meanwhile.
    fn start_control_picture(
        &mut self,
        ctx: &egui::Context,
        cmd: &serde_json::Value,
        reply: std::sync::mpsc::Sender<Result<serde_json::Value, String>>,
    ) -> Result<(), String> {
        if self.render_job.is_some() {
            let e = "A picture is already being drawn; wait for it to finish".to_owned();
            let _ = reply.send(Err(e.clone()));
            return Err(e);
        }
        let parsed = (|| -> Result<(std::path::PathBuf, crate::render_job::Work), String> {
            let kind = cmd.get("cmd").and_then(serde_json::Value::as_str).ok_or("Missing \"cmd\"")?;
            let path = cmd.get("path").and_then(serde_json::Value::as_str).filter(|p| !p.is_empty()).ok_or("Missing \"path\"")?;
            let size = cmd.get("size").and_then(serde_json::Value::as_u64);
            let work = if kind == "render" {
                crate::render_job::Work::Sheet { size: size.unwrap_or(220).clamp(40, 600) as usize }
            } else {
                crate::render_job::Work::Animation {
                    size: size.unwrap_or(240).clamp(64, 640) as usize,
                    gpu: cmd.get("gpu").and_then(serde_json::Value::as_bool).unwrap_or(false),
                }
            };
            Ok((std::path::PathBuf::from(path), work))
        })();
        let (path, work) = match parsed {
            Ok(parsed) => parsed,
            Err(e) => {
                let _ = reply.send(Err(e.clone()));
                return Err(e);
            }
        };
        // The worker owns its copy: edits made while the picture is drawn cannot mix into it.
        let scene = self.scene.clone();
        match crate::render_job::start(ctx, scene, path, work, reply.clone()) {
            Ok(job) => {
                self.render_job = Some(job);
                ctx.request_repaint();
                Ok(())
            }
            Err(e) => {
                let _ = reply.send(Err(e.clone()));
                Err(e)
            }
        }
    }

    /// Collect a finished control picture and forward its answer to the waiting client.
    fn poll_render_job(&mut self, ctx: &egui::Context) {
        let result = self.render_job.as_mut().and_then(crate::render_job::Job::poll);
        if let Some(result) = result {
            self.render_job = None;
            match result {
                Ok(value) => {
                    let path = value.get("png").or_else(|| value.get("gif")).and_then(serde_json::Value::as_str).unwrap_or_default();
                    self.say(ctx, format!("Saved {path}"));
                }
                Err(e) => self.say(ctx, format!("Picture not saved: {e}")),
            }
        } else if self.render_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    /// One frame of the window (what `eframe::App::ui` does; a method of its own so that tests can run it).
    pub(crate) fn frame_body(&mut self, ui: &mut Ui) {
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first ui frame");
        let ctx = ui.ctx().clone();
        self.poll_font(&ctx);
        if self.gpu_device_lost {
            return;
        }
        if let Some(lost) = self.gpu.rs.as_ref().map(|rs| rs.device_lost.clone()).filter(|lost| lost.is_lost()) {
            let reason = lost.reason().unwrap_or_default();
            crate::gpu_log::log(&format!("graphics device lost ({reason}); closing and preserving recovery data"));
            self.checkpoint_unsaved_for_device_loss();
            self.gpu_device_lost = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let frame_began = std::time::Instant::now();
        self.perf.stats.begin(ctx.input(|i| i.time));
        self.sync_precise_transfer(&ctx);
        self.poll_render_job(&ctx);
        self.run_control(&ctx);
        self.update_native_job(&ctx);
        self.handle_close_request(&ctx);
        self.poll_gif_job(&ctx);
        if self.precise_frame(ui) {
            self.test_hooks(&ctx);
            return;
        }
        self.fix_selection();
        if let Some(n) = self.notice.take() {
            self.say(&ctx, n);
        }
        self.keys(&ctx);
        let dt = ctx.input(|i| i.stable_dt);
        if self.step_fly(dt) {
            ctx.request_repaint();
        }
        self.top_bar(ui);
        self.status_bar(ui);
        self.anim_bar(ui);
        self.left_dock(ui);
        self.side(ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(52, 52, 56))).show(ui, |ui| {
            self.last_view_height = ui.available_height();
            self.viewport(ui);
        });
        if self.capture.is_none() {
            self.tool_strip(ui);
            self.draw_card(&ctx);
            self.pipe_card(&ctx);
            self.sketch_card(&ctx);
            if !self.guide.panel_open {
                self.keys_card(ui);
            }
            self.guide_card(&ctx);
            crate::svg_dialog::update(self, &ctx);
            crate::precise::transfer_card(self, &ctx);
            crate::puffy_text_ui::update(self, &ctx);
            crate::organic_blob::update(self, &ctx);
            crate::chess_dialog::update(self, &ctx);
            crate::mesh_dialog::update(self, &ctx);
            crate::array::update(self, &ctx);
            crate::export_dialog::update(self, &ctx);
            self.palette_ui(&ctx);
            crate::settings_ui::update(self, &ctx);
            crate::melt_preset::update(self, &ctx);
            if std::mem::take(&mut self.perf.bench_requested) {
                let backends = self.current_backend().map(|b| b.to_wgpu());
                self.bench.start(&ctx, backends);
            }
            self.bench.show(&ctx);
        }
        self.pending_file_dialog(&ctx);
        self.recovery_dialog(&ctx);
        self.native_job_card(&ctx);
        self.capture_step(&ctx);
        self.animation_capture_finished(&ctx);
        self.settle(&ctx);
        if !cfg!(test) {
            // Publish a checkpoint that finished on its worker, then consider a new one.
            // Serializing the scene never happens in this frame (only the worker does it).
            if let Some(result) = self.recovery.poll_checkpoint()
                && let Err(e) = result
            {
                self.notice = Some(format!("Could not save auto-recovery data: {e}"));
            }
            // Finding out whether the scene changed compares all of it, so it is only done when
            // a checkpoint is due (`start_checkpoint` would ignore the answer otherwise).
            let now = std::time::Instant::now();
            if self.recovery.due(now) {
                let rev = self.track_scene();
                let changed = self.has_unsaved_work();
                if let Err(e) = self.recovery.start_checkpoint(&self.scene, self.path.as_deref(), changed, rev, now) {
                    self.notice = Some(format!("Could not save auto-recovery data: {e}"));
                }
            }
            ctx.request_repaint_after(self.recovery.wait(now));
            if self.recovery.checkpoint_running() {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        self.test_hooks(&ctx);
        let title = format!("Melt 3D — {}{}", self.doc_name(), if self.dirty { " *" } else { "" });
        // Only when it changed: every viewport command wakes the event loop for another frame,
        // which used to keep the window drawing all the time.
        if title != self.perf.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.perf.last_title = title;
        }
        // something edited the scene after the viewport picture was made: draw once more
        let clean = self.capture.is_some();
        let rev = self.track_scene();
        if self.perf.drawn.wants_another_frame(rev, self.sel, self.hover, &self.cam, clean) {
            ctx.request_repaint();
        }
        self.perf.stats.end(frame_began.elapsed().as_secs_f32() * 1000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native open/save run on a worker: wait (bounded) until it has been applied.
    fn finish_file_jobs(app: &mut App, ctx: &egui::Context) {
        for _ in 0..800 {
            app.update_native_job(ctx);
            if app.native_job.is_none() && app.queued_open.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn pump_native(app: &mut App, ctx: &egui::Context) {
        for _ in 0..600 {
            app.update_native_job(ctx);
            if app.native_job.is_none() && app.queued_open.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("native document worker did not finish");
    }

    #[test]
    fn graphics_loss_exit_checkpoints_current_unsaved_scene() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("meltcad_device_loss_{stamp}"));
        let mut app = App::new(true, None);
        app.recovery = crate::recovery::Store::new(root.clone(), std::time::Instant::now());
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        let expected = app.scene.clone();

        app.checkpoint_unsaved_for_device_loss();
        app.gpu_device_lost = true;

        app.finish_recovery_on_exit();
        drop(app);

        let mut restarted = crate::recovery::Store::new(root.clone(), std::time::Instant::now());
        assert!(restarted.pending_path().is_some(), "a graphics-loss exit must leave the prior recovery checkpoint available");
        let (restored, _, _) = restarted.recover_pending().expect("read preserved recovery");
        assert_eq!(restored, expected, "the forced checkpoint must contain the current unsaved scene");
        restarted.normal_exit().expect("remove test lock");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn graphics_loss_does_not_create_recovery_for_a_clean_scene() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("meltcad_device_loss_clean_{stamp}"));
        let mut app = App::new(true, None);
        app.recovery = crate::recovery::Store::new(root.clone(), std::time::Instant::now());

        app.checkpoint_unsaved_for_device_loss();
        drop(app);

        let mut restarted = crate::recovery::Store::new(root.clone(), std::time::Instant::now());
        assert!(restarted.pending_path().is_none(), "a clean scene must not produce an empty recovery file");
        restarted.normal_exit().expect("remove test lock");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn undo_and_redo_invalidate_the_cpu_field_and_gpu_scene_pack_immediately() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let p = [25.0, 0.0, 20.0];
        let before = app.field();
        let dist_before = before.distance(p);
        let rev_before = app.scene_rev;
        let pack_before = app.perf.pack.get(&app.scene, rev_before);

        app.scene.shape_mut(0, 0).unwrap().pos[0] += 40.0;
        app.settle(&ctx);
        let rev_changed = app.scene_rev;
        assert!(rev_changed > rev_before);
        let after = app.field();
        let dist_changed = after.distance(p);
        assert_ne!(dist_changed, dist_before);
        let pack_changed = app.perf.pack.get(&app.scene, rev_changed);
        assert_ne!(pack_before.id, pack_changed.id);

        app.undo();
        assert!(app.scene_rev > rev_changed);
        assert_eq!(app.field().distance(p).to_bits(), dist_before.to_bits());
        let undo_pack = app.perf.pack.get(&app.scene, app.scene_rev);
        assert_eq!(undo_pack.list.as_ref(), pack_before.list.as_ref());
        app.redo();
        assert_eq!(app.field().distance(p).to_bits(), dist_changed.to_bits());
        let redo_pack = app.perf.pack.get(&app.scene, app.scene_rev);
        assert_eq!(redo_pack.list.as_ref(), pack_changed.list.as_ref());
        assert!(app.field_builds.get() >= 4, "each scene revision must refresh the SDF");
    }

    #[test]
    fn adding_undoing_and_deleting_shapes() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        assert_eq!(app.scene.shape_count(), 1);
        app.add_shape(&ctx, Kind::Box);
        assert_eq!(app.scene.shape_count(), 2);
        assert_eq!(app.sel, Some(Pick::Shape(0, 1)));
        app.settle(&ctx);
        assert_eq!(app.undo.len(), 1, "one finished edit is one undo step");
        app.undo();
        assert_eq!(app.scene.shape_count(), 1);
        app.redo();
        assert_eq!(app.scene.shape_count(), 2);
        app.sel = Some(Pick::Shape(0, 1));
        app.delete_selected();
        assert_eq!(app.scene.shape_count(), 1);
        assert_eq!(app.sel, Some(Pick::Shape(0, 0)));
    }

    #[test]
    fn delete_key_removes_selected_shape_when_sketch_has_no_draft() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        assert_eq!(app.sel, Some(Pick::Shape(0, 0)));
        assert!(app.sk.pts.is_empty() && app.sk.anchor.is_none());

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert_eq!(app.scene.shape_count(), 0, "Delete should remove the selected shape when no sketch points are pending");
    }

    #[test]
    fn backspace_removes_pending_sketch_point_without_deleting_shape() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.click([0.0, 0.0], false);
        assert_eq!(app.sk.pts.len(), 1);

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert!(app.sk.pts.is_empty());
        assert_eq!(app.scene.shape_count(), 1);
    }

    #[test]
    fn delete_removes_pending_sketch_point_without_deleting_shape() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.click([0.0, 0.0], false);
        assert_eq!(app.sk.pts.len(), 1);

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert!(app.sk.pts.is_empty());
        assert_eq!(app.scene.shape_count(), 1);
    }

    #[test]
    fn delete_cancels_sketch_anchor_without_deleting_shape() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.pen = crate::sketch_ui::Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        assert_eq!(app.sk.anchor, Some([0.0, 0.0]));

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert!(app.sk.anchor.is_none());
        assert_eq!(app.scene.shape_count(), 1);
    }

    #[test]
    fn delete_in_outline_edit_without_chosen_point_keeps_the_shape() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.pen = crate::sketch_ui::Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        let profile = app.sk.click([20.0, 20.0], false).expect("rectangle profile");
        app.sketch_done(profile);
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("new solid selected") };
        app.sketch_edit_begin(g, s);
        assert!(app.sk.edit.as_ref().is_some_and(|edit| edit.sel.is_none()));
        let before = app.scene.shape_count();

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert_eq!(app.scene.shape_count(), before, "Delete without a chosen outline point must not remove the solid");
        assert_eq!(app.sel, Some(Pick::Shape(g, s)));
    }

    #[test]
    fn new_scene_waits_for_unsaved_work_to_be_resolved() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        assert!(app.dirty);

        app.new_scene(Scene::default());

        assert_eq!(app.scene.shape_count(), 2, "New must leave the edited work in place until a choice is made");
        assert!(app.dirty);
        assert!(app.pending_file_action.is_some());
        app.resolve_pending_file_action(&ctx, PendingFileDecision::Cancel);
        assert_eq!(app.scene.shape_count(), 2);
        assert!(app.pending_file_action.is_none());
    }

    #[test]
    fn opening_another_file_waits_for_unsaved_work_to_be_resolved() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        let path = std::env::temp_dir().join(format!("meltcad_open_guard_{}.meltcad", std::process::id()));
        std::fs::write(&path, Scene::default().to_json()).expect("sample file");

        app.open_path(path.clone());

        assert_eq!(app.scene.shape_count(), 2, "Open must leave the edited work in place until a choice is made");
        assert!(app.pending_file_action.is_some());
        app.resolve_pending_file_action(&ctx, PendingFileDecision::Discard);
        assert_eq!(app.scene.shape_count(), 2, "keep current scene until parsing finishes");
        pump_native(&mut app, &ctx);
        assert_eq!(app.scene.shape_count(), 1);
        assert_eq!(app.path.as_ref(), Some(&path));
        assert!(!app.dirty);
        std::fs::remove_file(path).expect("remove sample file");
    }

    #[test]
    fn failed_save_does_not_replace_unsaved_work() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        app.path = Some(std::env::temp_dir().join(format!("meltcad_missing_{}", std::process::id())).join("scene.meltcad"));
        app.new_scene(Scene::default());

        app.resolve_pending_file_action(&ctx, PendingFileDecision::Save);
        pump_native(&mut app, &ctx);

        assert_eq!(app.scene.shape_count(), 2);
        assert!(app.dirty);
        assert!(app.pending_file_action.is_some(), "failed save must not discard the pending action");
    }

    #[test]
    fn undoing_past_a_save_keeps_the_document_marked_as_changed() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let path = std::env::temp_dir().join(format!("meltcad_undo_after_save_{}.meltcad", std::process::id()));
        app.path = Some(path.clone());
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        assert!(app.try_save(&ctx, false), "the save starts");
        finish_file_jobs(&mut app, &ctx);
        assert!(!app.dirty, "a finished save starts clean");

        app.undo();

        assert_eq!(app.scene.shape_count(), 1, "the undo removed the saved shape");
        assert!(app.dirty, "the file on disk still has the edit the undo took back");
        assert!(app.has_unsaved_work(), "closing must ask before the undone state is thrown away");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cancelling_or_failing_native_open_leaves_live_scene_unchanged() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        let dir = std::env::temp_dir().join(format!("yura_doc_cancel_app_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let invalid = dir.join("invalid.yura");
        std::fs::write(&invalid, b"{invalid JSON").expect("fixture");
        app.open_path(invalid);
        pump_native(&mut app, &ctx);
        assert_eq!(app.scene, before, "failure must never replace a design");

        let valid = dir.join("valid.yura");
        std::fs::write(&valid, Scene::default().to_json()).expect("fixture");
        // the worker waits, so the job is still running when the test looks at it
        let hold = crate::document_job::hold_open_workers(&ctx);
        app.open_path(valid);
        app.update_native_job(&ctx);
        let job = app.native_job.as_mut().expect("started load");
        assert!(job.cancel(), "the load is cancellable");
        hold.store(false, std::sync::atomic::Ordering::Release);
        pump_native(&mut app, &ctx);
        assert_eq!(app.scene, before, "cancelled results must never replace a design");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_heavy_worker_blocks_parallel_loading_without_blocking_the_ui() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let dir = std::env::temp_dir().join(format!("yura_budget_open_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("valid.yura");
        std::fs::write(&path, Scene::default().to_json()).expect("fixture");
        let permit = crate::work_budget::try_acquire(&ctx).expect("other heavy job");
        app.open_path(path.clone());
        app.update_native_job(&ctx);
        assert!(app.native_job.is_none() && app.queued_open.is_some(), "read is queued instead of allocating another large job");
        assert!(app.scene.shape_count() > 0, "the existing model remains usable");
        drop(permit);
        pump_native(&mut app, &ctx);
        assert_eq!(app.path.as_ref(), Some(&path));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edits_while_an_open_waits_for_the_worker_are_preserved() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let dir = std::env::temp_dir().join(format!("yura_queued_open_edit_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("replacement.yura");
        std::fs::write(&path, Scene::default().to_json()).expect("fixture");
        let permit = crate::work_budget::try_acquire(&ctx).expect("other heavy job");

        app.open_path(path);
        app.update_native_job(&ctx);
        assert!(app.queued_open.is_some(), "open waits behind the existing worker");
        app.add_shape(&ctx, Kind::Box);
        let edited = app.scene.clone();
        drop(permit);

        pump_native(&mut app, &ctx);
        assert_eq!(app.scene, edited, "the queued open must not discard edits made while it waited");
        assert!(app.path.is_none(), "the rejected open must not change the current document path");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// #232: a running GIF export keeps the window from closing under it.
    #[test]
    fn closing_while_a_gif_is_being_made_is_refused() {
        use std::sync::atomic::{AtomicBool, AtomicU32};
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let (_tx, rx) = std::sync::mpsc::channel();
        app.an.set_job_for_test(crate::anim_ui::Job::for_test(
            rx,
            std::sync::Arc::new(AtomicU32::new(0)),
            std::sync::Arc::new(AtomicU32::new(1)),
            std::sync::Arc::new(AtomicBool::new(false)),
        ));
        assert!(app.an.gif_running());
        let mut input = egui::RawInput::default();
        input.viewports.get_mut(&egui::ViewportId::ROOT).expect("root viewport").events.push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |_ui| app.handle_close_request(&ctx));
        output.textures_delta.clear();
        assert!(output.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose)));
        assert!(!app.close_approved);
    }

    #[test]
    fn closing_with_unsaved_work_asks_before_exiting() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        let mut input = egui::RawInput::default();
        input.viewports.get_mut(&egui::ViewportId::ROOT).expect("root viewport").events.push(egui::ViewportEvent::Close);

        let mut output = ctx.run_ui(input, |_ui| app.handle_close_request(&ctx));

        assert!(matches!(app.pending_file_action, Some(PendingFileAction::Close)));
        assert!(output.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose)));
        output.textures_delta.clear();
        app.resolve_pending_file_action(&ctx, PendingFileDecision::Cancel);
        assert!(app.pending_file_action.is_none());
        assert_eq!(app.scene.shape_count(), 2);
    }

    #[test]
    fn control_mode_exit_uses_the_automatic_solid_import_and_rejects_invalid_input() {
        use serde_json::json;
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let (sender, receiver) = std::sync::mpsc::channel();
        app.control = Some(receiver);
        let (reply, results) = std::sync::mpsc::channel();
        sender.send(crate::control::Request { cmd: json!({"cmd":"ui.precise","on":"false"}), reply: reply.clone() }).unwrap();
        app.run_control(&ctx);
        assert!(results.recv().unwrap().is_err());
        assert!(!app.precise_on);
        sender.send(crate::control::Request { cmd: json!({"cmd":"ui.precise","on":true}), reply: reply.clone() }).unwrap();
        app.run_control(&ctx);
        assert_eq!(results.recv().unwrap().unwrap()["precise"], true);
        let p = app.precise.as_mut().unwrap();
        assert_eq!(p.app.preferences.language, "ja");
        p.app.run("solid.box", json!({"length":24,"width":12,"height":6})).unwrap();
        let scene = app.scene.clone();
        sender.send(crate::control::Request { cmd: json!({"cmd":"scene.new"}), reply: reply.clone() }).unwrap();
        app.run_control(&ctx);
        assert!(results.recv().unwrap().is_err(), "do not change a hidden Melt scene while SolidCraft is active");
        assert_eq!(app.scene, scene);
        sender.send(crate::control::Request { cmd: json!({"cmd":"ui.precise","on":false}), reply }).unwrap();
        app.run_control(&ctx);
        let answer = results.recv().unwrap().unwrap();
        assert_eq!(answer["precise"], false);
        assert_eq!(answer["importing"], true);
        assert!(app.precise_transfer_pending());
        // The STL is written and parsed on a worker; the bake card opens when it is ready.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !crate::mesh_dialog::is_open(&ctx) {
            app.sync_precise_transfer(&ctx);
            assert!(std::time::Instant::now() < deadline, "the transfer did not reach the bake");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(crate::mesh_dialog::cancel(&ctx));
        app.sync_precise_transfer(&ctx);
    }

    #[test]
    fn closing_from_either_mode_protects_the_unsaved_exact_document() {
        for precise_on in [true, false] {
            let ctx = egui::Context::default();
            let mut app = App::new(true, None);
            app.open_precise(&ctx);
            let p = app.precise.as_mut().unwrap();
            p.app.run("solid.box", serde_json::json!({"length":24,"width":12,"height":6})).unwrap();
            app.precise_on = precise_on;
            let mut input =
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 600.0))), ..Default::default() };
            input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().events.push(egui::ViewportEvent::Close);
            let mut output = ctx.run_ui(input, |ui| app.frame_body(ui));
            assert!(output.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose)));
            assert!(app.precise_on, "show the precise document's save prompt even when it was hidden");
            let p = app.precise.as_ref().unwrap();
            assert!(p.app.docs.quitting.is_some() && p.app.docs.closing.is_some());
            assert!(!p.app.quit_requested && p.app.session.is_dirty());
            output.textures_delta.clear();
        }
    }

    #[test]
    fn launching_into_solidcraft_without_edits_closes_without_a_prompt() {
        assert!(!crate::control::starts_in_solidcraft(None));
        assert!(!crate::control::starts_in_solidcraft(Some(std::path::Path::new("a.meltcad"))));
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.open_precise(&ctx);
        assert!(app.precise_on);
        let mut input =
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 600.0))), ..Default::default() };
        input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().events.push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| app.frame_body(ui));
        output.textures_delta.clear();
        assert!(!output.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose)));
    }

    #[test]
    fn the_melt_button_melts_picked_groups_into_each_other_also_inside_a_big_group() {
        let ball = |x: f32| Shape::new(Kind::Sphere, [x, 0.0, 0.0], crate::model::Material::default());
        for nested in [false, true] {
            let mut app = App::new(true, None);
            let mut a = Group::new("A");
            a.shapes = vec![ball(-14.0)];
            let mut b = Group::new("B");
            b.shapes = vec![ball(14.0)];
            app.scene.groups = vec![a, b];
            app.committed = app.scene.clone();
            let gap = |app: &App| crate::sdf::Field::new(&app.scene).distance([0.0, 0.0, 0.0]);
            app.sel = Some(Pick::Group(0));
            app.multi = vec![Pick::Group(1)];
            app.multi_for = app.sel;
            if nested {
                app.group_selected();
                assert_eq!(app.scene.groups.len(), 3);
                assert_eq!(app.scene.groups[0].name, "Parent Group 1", "a group of groups says what it is");
                assert_eq!(app.scene.groups.iter().map(|g| g.depth).collect::<Vec<_>>(), vec![0, 1, 1]);
                app.sel = Some(Pick::Group(1));
                app.multi = vec![Pick::Group(2)];
                app.multi_for = app.sel;
            }
            let hard = gap(&app);
            app.toggle_melt_selected();
            let melted = gap(&app);
            assert!(melted < hard - 0.5, "nested {nested}: the two groups melt into each other ({melted} vs {hard})");
            app.toggle_melt_selected();
            assert!((gap(&app) - hard).abs() < 1e-3, "nested {nested}: pressing again takes the melt away");
        }
    }

    #[test]
    fn melt_off_on_the_only_shape_of_a_group_stops_the_group_melting_too() {
        let ball = |x: f32| {
            let mut s = Shape::new(Kind::Box, [x, 0.0, 0.0], crate::model::Material::default());
            s.blend = 6.0;
            s
        };
        let mut app = App::new(true, None);
        let mut a = Group::new("A");
        a.shapes = vec![ball(-21.0)];
        let mut b = Group::new("B");
        b.blend = 12.0;
        b.shapes = vec![ball(21.0)];
        app.scene.groups = vec![a, b];
        app.sel = Some(Pick::Shape(1, 0));
        assert!(app.selection_melts());
        app.toggle_melt_selected();
        assert!(!app.selection_melts());
        assert_eq!(app.scene.groups[1].blend, 0.0, "B held only that shape: its melt into A is off too (#236)");
        // a group that still holds another melting shape keeps its melt
        let mut c = ball(60.0);
        c.blend = 6.0;
        app.scene.groups[1].shapes.push(c);
        app.scene.groups[1].blend = 12.0;
        app.scene.groups[1].shapes[0].blend = 6.0;
        app.sel = Some(Pick::Shape(1, 0));
        app.toggle_melt_selected();
        assert_eq!(app.scene.groups[1].blend, 12.0);
        assert_eq!(app.scene.groups[1].shapes[1].blend, 6.0);
    }

    #[test]
    fn melting_shapes_never_melts_their_groups_into_each_other() {
        let ball = |x: f32| {
            let mut s = Shape::new(Kind::Box, [x, 0.0, 0.0], crate::model::Material::default());
            s.blend = 0.0;
            s
        };
        let mut app = App::new(true, None);
        let mut a = Group::new("A");
        a.shapes = vec![ball(-21.0), ball(-5.0)];
        let mut b = Group::new("B");
        b.shapes = vec![ball(21.0)];
        app.scene.groups = vec![a, b];
        // a shape picked: its own melt turns on, its group stays hard against the other groups
        app.sel = Some(Pick::Shape(1, 0));
        app.toggle_melt_selected();
        assert!(app.scene.groups[1].shapes[0].blend > 0.0);
        assert_eq!(app.scene.groups[1].blend, 0.0, "different groups do not melt unless the group itself is melted");
        // nothing picked: the whole scene melts inside the groups only
        app.sel = None;
        app.multi.clear();
        app.toggle_melt_selected();
        assert!(app.scene.groups.iter().all(|g| g.blend == 0.0));
        // a picked group row melts into the groups before it on purpose
        app.sel = Some(Pick::Group(1));
        app.toggle_melt_selected();
        assert!(app.scene.groups[1].blend > 0.0);
    }

    #[test]
    fn display_units_and_decimals_shape_the_status_size() {
        let mut app = App::new(true, None);
        let mut b = Shape::new(Kind::Box, [0.0; 3], crate::model::Material::default());
        b.size = [20.0, 5.0, 10.0];
        app.scene.groups[0].shapes = vec![b];
        app.sel = Some(Pick::Shape(0, 0));
        app.preferences.units = "cm".into();
        app.preferences.decimals = 2;
        app.apply_preferences();
        assert_eq!(app.selection_size().as_deref(), Some("W 2.00 × D 0.50 × H 1.00 cm"));
        app.preferences.units = "in".into();
        app.preferences.decimals = 1;
        app.apply_preferences();
        assert_eq!(app.selection_size().as_deref(), Some("W 0.8 × D 0.2 × H 0.4 inch"));
    }

    #[test]
    fn autosave_preference_moves_the_next_checkpoint() {
        let mut app = App::new(true, None);
        let before = std::time::Instant::now();
        app.preferences.autosave_minutes = 5.0;
        app.apply_preferences();
        let wait = app.recovery.wait(before);
        assert!(wait >= std::time::Duration::from_secs(299), "5 minutes are used, not the 30 second default: {wait:?}");
        assert!(wait <= std::time::Duration::from_secs(301), "{wait:?}");
    }

    #[test]
    fn a_quarter_turned_box_shows_its_sizes_along_the_world_axes() {
        let mut app = App::new(true, None);
        let mut b = Shape::new(Kind::Box, [0.0; 3], crate::model::Material::default());
        b.size = [20.0, 5.0, 10.0];
        let h = b.local_half();
        b.rot = [0.0, 0.0, 90.0];
        app.scene.groups[0].shapes = vec![b];
        app.sel = Some(Pick::Shape(0, 0));
        let text = app.selection_size().expect("a size");
        let want = format!("W {:.1} × D {:.1} × H {:.1} mm", h[1] * 2.0, h[0] * 2.0, h[2] * 2.0);
        assert_eq!(text, want, "a quarter turn about Z swaps width and depth (#195)");
    }

    #[test]
    fn a_duplicated_parent_group_links_its_own_copy_not_the_original() {
        let mut app = App::new(true, None);
        let mut p = Group::new("P");
        p.uid = 0;
        let mut a = Group::new("A");
        a.depth = 1;
        a.uid = 5;
        a.shapes.push(Shape::new(Kind::Sphere, [0.0; 3], crate::model::Material::default()));
        let mut b = Group::new("B");
        b.depth = 1;
        b.instance_of = Some(5);
        app.scene.groups = vec![p, a, b];
        app.sel = Some(Pick::Group(0));
        app.duplicate();
        assert_eq!(app.scene.groups.len(), 6);
        let (a2, b2) = (&app.scene.groups[4], &app.scene.groups[5]);
        assert_ne!(a2.uid, 5, "the copy has a number of its own");
        assert_eq!(b2.instance_of, Some(a2.uid), "the copied link shows the copied group (#202)");
        assert_eq!(app.scene.groups[2].instance_of, Some(5), "the original link is untouched");
    }

    #[test]
    fn dragging_a_parent_group_beside_a_nested_group_keeps_its_tree() {
        let mut app = App::new(true, None);
        let g = |n: &str, d: u8| {
            let mut x = Group::new(n);
            x.depth = d;
            x
        };
        app.scene.groups = vec![g("A", 0), g("a1", 1), g("B", 0), g("b1", 1)];
        let before: Vec<(String, u8)> = app.scene.groups.iter().map(|x| (x.name.clone(), x.depth)).collect();
        app.move_layer(Pick::Group(0), Pick::Group(3), true);
        let after: Vec<(String, u8)> = app.scene.groups.iter().map(|x| (x.name.clone(), x.depth)).collect();
        assert_eq!(after, before, "a parent group cannot land among nested groups; nothing is flattened (#203)");
    }

    #[test]
    fn a_shape_dropped_on_the_line_above_a_group_joins_the_group_above() {
        let mut app = App::new(true, None);
        let sp = || Shape::new(Kind::Sphere, [0.0; 3], crate::model::Material::default());
        let mut a = Group::new("A");
        a.shapes = vec![sp()];
        let mut b = Group::new("B");
        b.shapes = vec![sp(), sp()];
        app.scene.groups = vec![a, b];
        let c = Group::new("C");
        app.scene.groups.push(c);
        // dropped on the line above C, which is the end of B
        app.move_layer(Pick::Shape(0, 0), Pick::Group(2), false);
        assert_eq!(app.scene.groups[1].shapes.len(), 3, "it joined B, the group above the line (#279)");
        assert_eq!(app.scene.groups[2].shapes.len(), 0, "and did not go into C below the line");
        assert_eq!(app.scene.groups[0].shapes.len(), 0);
        // on the lower edge of a group row it becomes that group's first shape
        app.move_layer(Pick::Shape(1, 2), Pick::Group(2), true);
        assert_eq!(app.scene.groups[2].shapes.len(), 1);
    }

    #[test]
    fn several_picked_rows_go_into_a_group_together() {
        let mut app = App::new(true, None);
        let sp = || Shape::new(Kind::Sphere, [0.0; 3], crate::model::Material::default());
        let mut a = Group::new("A");
        a.shapes = vec![sp(), sp()];
        let mut b = Group::new("B");
        b.shapes = vec![sp()];
        let c = Group::new("C");
        let d = Group::new("D");
        app.scene.groups = vec![a, b, c, d];
        let picks = [Pick::Shape(0, 0), Pick::Shape(0, 1), Pick::Group(1), Pick::Group(2)];
        app.move_picks_into(&picks, 3).unwrap();
        assert_eq!(app.scene.groups[0].shapes.len(), 0, "A lost both shapes");
        let names: Vec<(&str, u8)> = app.scene.groups.iter().map(|g| (g.name.as_str(), g.depth)).collect();
        assert_eq!(names, vec![("A", 0), ("D", 0), ("B", 1), ("C", 1)], "B and C are inside D");
        assert_eq!(app.scene.groups[1].shapes.len(), 2, "the shapes joined D");
        assert!(app.move_picks_into(&[Pick::Group(1)], 2).is_err(), "a group cannot go into what is inside it");
    }

    #[test]
    fn a_failed_open_long_after_start_still_shows_why() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.apply_file_action(PendingFileAction::Open(std::path::PathBuf::from("/nonexistent/melt3d/missing.meltcad")));
        for _ in 0..400 {
            let input = egui::RawInput { time: Some(60.0), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| app.frame_body(ui));
            out.textures_delta.clear();
            if app.native_job.is_none() && app.queued_open.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.status.0.contains("Could not open"), "{:?}", app.status);
        assert!((app.status.1 - 60.0).abs() < 1.0, "shown at the time it happened, not at start-up (#219): {:?}", app.status);
    }

    #[test]
    fn saving_before_reopening_the_current_file_keeps_the_saved_edit() {
        let path = std::env::temp_dir().join(format!("meltcad_reopen_guard_{}.meltcad", std::process::id()));
        std::fs::write(&path, Scene::default().to_json()).expect("initial file");
        let ctx = egui::Context::default();
        let mut app = App::new(true, Some(path.clone()));
        pump_native(&mut app, &ctx);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);

        app.open_path(path.clone());
        app.resolve_pending_file_action(&ctx, PendingFileDecision::Save);
        pump_native(&mut app, &ctx);

        assert_eq!(app.scene.shape_count(), 2, "the just-saved edit must still be on screen");
        let disk = std::fs::read_to_string(&path).expect("saved file");
        assert_eq!(Scene::from_json_notes(&disk).expect("saved scene").0.shape_count(), 2);
        assert!(!app.dirty);
        std::fs::remove_file(path).expect("remove sample file");
    }

    #[test]
    fn pending_file_decision_blocks_other_shortcuts_and_file_actions() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);
        app.new_scene(Scene::default());
        let path = std::env::temp_dir().join(format!("meltcad_pending_guard_{}.meltcad", std::process::id()));
        std::fs::write(&path, Scene::default().to_json()).expect("other file");

        app.open_path(path.clone());
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.keys(&ctx);

        assert!(matches!(app.pending_file_action, Some(PendingFileAction::New(_))), "the first decision stays pending");
        assert_eq!(app.scene.shape_count(), 2, "Delete cannot edit behind the dialog");
        std::fs::remove_file(path).expect("remove sample file");
    }

    #[test]
    fn delete_cannot_edit_behind_the_mesh_or_export_card() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        let n = app.scene.shape_count();
        for open in [
            (|c: &egui::Context| crate::mesh_dialog::open_path(c, std::env::temp_dir().join("meltcad-no-such-mesh.stl"), 0)) as fn(&egui::Context),
            |c| crate::export_dialog::open(c, None),
        ] {
            open(&ctx);
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
            });
            app.keys(&ctx);
            assert_eq!(app.scene.shape_count(), n, "Delete belongs to the open card, not the shape behind it");
        }
    }

    #[test]
    fn minus_shapes_carve_and_plus_minus_flips() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.new_op = Op::Subtract;
        app.add_shape(&ctx, Kind::Cylinder);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        assert_eq!(app.scene.shape(g, s).map(|x| x.op), Some(Op::Subtract));
        app.flip_op();
        assert_eq!(app.scene.shape(g, s).map(|x| x.op), Some(Op::Add));
    }

    #[test]
    fn shift_click_selects_several_and_they_group_move_and_delete_together() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Sphere);
        app.add_shape(&ctx, Kind::Box);
        app.add_shape(&ctx, Kind::Cylinder);
        let all: Vec<Pick> = (0..app.scene.groups.iter().map(|g| g.shapes.len()).sum::<usize>())
            .filter_map(|n| {
                let mut left = n;
                for (g, gr) in app.scene.groups.iter().enumerate() {
                    if left < gr.shapes.len() {
                        return Some(Pick::Shape(g, left));
                    }
                    left -= gr.shapes.len();
                }
                None
            })
            .collect();
        assert!(all.len() >= 3);
        app.sel = Some(all[0]);
        app.toggle_pick(all[1]);
        app.toggle_pick(all[2]);
        assert_eq!(app.picks().len(), 3);
        // Shift on one that is already in takes it out
        app.toggle_pick(all[1]);
        assert_eq!(app.picks(), vec![all[0], all[2]]);
        // a plain selection elsewhere drops the extras
        app.sel = Some(all[1]);
        assert_eq!(app.picks(), vec![all[1]]);
        // moving by the arrow keys moves all of them
        app.sel = Some(all[0]);
        app.multi.clear();
        app.toggle_pick(all[2]);
        let before: Vec<f32> = app.sel_targets().iter().filter_map(|(g, s)| app.scene.shape(*g, *s).map(|x| x.pos[0])).collect();
        app.nudge([5.0, 0.0, 0.0]);
        let after: Vec<f32> = app.sel_targets().iter().filter_map(|(g, s)| app.scene.shape(*g, *s).map(|x| x.pos[0])).collect();
        assert!(before.iter().zip(&after).all(|(b, a)| (a - b - 5.0).abs() < 1e-4));
        // group them: one new group holding exactly those two
        let total = app.scene.shape_count();
        app.group_selected();
        let Some(Pick::Group(g)) = app.sel else { panic!("the new group is selected") };
        assert_eq!(app.scene.groups[g].shapes.len(), 2);
        assert_eq!(app.scene.shape_count(), total);
        // delete the new group with another shape picked too
        app.delete_selected();
        assert_eq!(app.scene.shape_count(), total - 2);
    }

    #[test]
    fn the_scene_never_grows_past_its_limits() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        for _ in 0..200 {
            app.add_shape(&ctx, Kind::Sphere);
            app.add_group();
        }
        assert!(app.scene.shape_count() <= MAX_SHAPES && app.scene.groups.len() <= MAX_GROUPS);
    }

    #[test]
    fn a_mirror_copy_sits_on_the_other_side() {
        let mut app = App::new(true, None);
        if let Some(s) = app.scene.shape_mut(0, 0) {
            s.pos = [12.0, 3.0, 5.0];
        }
        app.mirror_copy();
        assert_eq!(app.scene.shape(0, 1).map(|s| s.pos), Some([-12.0, 3.0, 5.0]));
    }

    #[test]
    fn layers_can_be_dragged_into_a_new_order() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.add_shape(&ctx, Kind::Cone);
        let names = |a: &App| a.scene.groups[0].shapes.iter().map(|s| s.kind).collect::<Vec<_>>();
        assert_eq!(names(&app), vec![Kind::Sphere, Kind::Box, Kind::Cone]);
        app.move_layer(Pick::Shape(0, 2), Pick::Shape(0, 0), false);
        assert_eq!(names(&app), vec![Kind::Cone, Kind::Sphere, Kind::Box]);
        app.move_layer(Pick::Shape(0, 0), Pick::Shape(0, 2), true);
        assert_eq!(names(&app), vec![Kind::Sphere, Kind::Box, Kind::Cone]);
        app.add_group();
        app.move_layer(Pick::Shape(0, 1), Pick::Group(1), true);
        assert_eq!((app.scene.groups[0].shapes.len(), app.scene.groups[1].shapes.len()), (2, 1));
        app.move_layer(Pick::Group(1), Pick::Group(0), false);
        assert_eq!(app.scene.groups[0].shapes.len(), 1);
    }

    #[test]
    fn scene_rows_name_their_parent_in_the_tooltip() {
        let empty = |name: &str, depth: u8| {
            let mut g = Group::new(name);
            g.depth = depth;
            g
        };
        let mut scene = Scene { groups: vec![empty("Root", 0), empty("Child", 1), empty("Grandchild", 2)], ..Scene::default() };
        assert_eq!(crate::panels::row_parent_tip(&scene, Pick::Group(0)), None, "a top-level group has no parent to name");
        assert_eq!(crate::panels::row_parent_tip(&scene, Pick::Group(1)).as_deref(), Some("in Root"));
        assert_eq!(crate::panels::row_parent_tip(&scene, Pick::Group(2)).as_deref(), Some("in Root › Child"));
        // A shape names the group that holds it, with the same chain for equal names.
        scene.groups[2].shapes.push(Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        assert_eq!(crate::panels::row_parent_tip(&scene, Pick::Shape(2, 0)).as_deref(), Some("in Root › Child › Grandchild"));
    }

    #[test]
    fn three_level_scene_tree_indents_and_draws_a_guide_per_level() {
        for level in 0..8u8 {
            assert!(crate::panels::tree_indent(level + 1) > crate::panels::tree_indent(level), "each level must move right of the one before it");
            assert!(crate::panels::tree_guide_x(level + 1) > crate::panels::tree_guide_x(level), "each level needs its own guide column");
            assert!(crate::panels::tree_guide_color(level).a() >= 96, "deep guides must stay readable");
        }

        let ctx = egui::Context::default();
        crate::fonts::setup(&ctx);
        style::apply(&ctx);
        ctx.set_pixels_per_point(1.0);
        let mut app = App::new(true, None);
        let group = |name: &str, depth: u8| {
            let mut g = Group::new(name);
            g.depth = depth;
            g.shapes.push(Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
            g
        };
        app.scene.groups = vec![group("Root", 0), group("Child", 1), group("Grandchild", 2)];
        app.sel = None;
        let size = egui::vec2(360.0, 240.0);
        let mut tex = style::preview::Textures::new();
        let mut out = None;
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
            let mut frame = ctx.run_ui(input, |ui| app.layers(ui));
            style::preview::apply_deltas(&mut tex, &frame.textures_delta);
            frame.textures_delta.clear();
            out = Some(frame);
        }
        let Some(out) = out else { panic!("the scene list laid out") };
        let (w, h) = (size.x as usize, size.y as usize);
        let img = style::preview::raster(&ctx.tessellate(out.shapes, 1.0), &tex, w, h, 1.0, [0.12, 0.12, 0.13]);
        let rect = |pick: Pick| app.row_rects.iter().find(|(p, _)| *p == pick).map(|(_, r)| *r).expect("row was laid out");
        let (g0, g1, g2) = (rect(Pick::Group(0)), rect(Pick::Group(1)), rect(Pick::Group(2)));
        let (x0, x1, x2) =
            (g0.left() + crate::panels::tree_guide_x(0), g1.left() + crate::panels::tree_guide_x(1), g2.left() + crate::panels::tree_guide_x(2));
        assert!(x0 < x1 && x1 < x2, "three levels must have three increasing guide columns");
        let brightest = |x: f32, y: f32| {
            let (xi, yi) = (x.round() as isize, y.round() as isize);
            let mut best = 0.0_f32;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(px) = img.get((yi + dy) as usize * w + (xi + dx) as usize) {
                        best = best.max(px[0].max(px[1]).max(px[2]));
                    }
                }
            }
            best
        };
        // Sample just below each group's own row centre: every ancestor's line passes there.
        let bg = 0.12;
        for (x, y) in
            [(x0, g1.center().y + 8.0), (x1, g1.center().y + 8.0), (x0, g2.center().y + 8.0), (x1, g2.center().y + 8.0), (x2, g2.center().y + 8.0)]
        {
            assert!(brightest(x, y) > bg + 0.08, "no tree guide at {x},{y}");
        }
    }

    #[test]
    fn folding_a_parent_hides_its_rows_and_unfolding_brings_them_back() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let mut root = Group::new("Root");
        root.shapes.push(Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        let mut child = Group::new("Child");
        child.depth = 1;
        child.shapes.push(Shape::new(Kind::Box, [0.0; 3], Material::default()));
        app.scene.groups = vec![root, child];
        let run = |app: &mut App| {
            let input =
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 200.0))), ..Default::default() };
            let mut frame = ctx.run_ui(input, |ui| app.layers(ui));
            frame.textures_delta.clear();
        };
        run(&mut app);
        let shown = |app: &App| app.row_rects.iter().map(|(p, _)| *p).collect::<Vec<_>>();
        assert!(shown(&app).contains(&Pick::Shape(0, 0)) && shown(&app).contains(&Pick::Shape(1, 0)));
        app.folded.insert(0);
        run(&mut app);
        assert_eq!(shown(&app), vec![Pick::Group(0)], "a folded group hides its shapes and nested groups");
        app.folded.remove(&0);
        run(&mut app);
        assert!(shown(&app).contains(&Pick::Group(1)) && shown(&app).contains(&Pick::Shape(1, 0)));
    }

    #[test]
    fn a_new_group_beside_an_equal_name_gets_a_numbered_default() {
        let mut app = App::new(true, None);
        let empty = |name: &str| {
            let mut g = Group::new(name);
            g.shapes.clear();
            g
        };
        // The next default "Group 3" already exists (a renamed or wrapped group), so the new
        // group must not repeat it; the existing group keeps its name.
        app.scene.groups = vec![empty("Group 1"), empty("Group 3")];
        app.add_group();
        let names: Vec<&str> = app.scene.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Group 1", "Group 3", "Group 3 (2)"]);
        // Wrapping a group gets the same treatment through the same helper.
        let base = format!("Group {}", app.scene.groups.len() + 1);
        let name = app.scene.unique_group_name(&base);
        assert_eq!(name, "Group 4");
        app.scene.groups.push(Group::new("Group 4"));
        assert_eq!(app.scene.unique_group_name(&base), "Group 4 (2)");
        assert_eq!(app.scene.groups[1].name, "Group 3", "existing names are never rewritten");
    }

    #[test]
    fn a_group_can_be_duplicated_and_the_copy_is_selected() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Group(0));
        app.duplicate();
        assert_eq!(app.scene.groups.len(), 2);
        assert_eq!(app.sel, Some(Pick::Group(1)));
    }

    #[test]
    fn replacing_a_document_clears_live_shape_and_node_edit_indices() {
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.keep_sketch = false;
        app.sketch_done(crate::sketch_ui::Finished::Profile(crate::sketch::rect([0.0, 0.0], [20.0, 10.0])));
        let (g, s) = app.sk.live.expect("preview");
        let mut replacement = app.scene.clone();
        replacement.groups[g].shapes[s].name = "New document's independent body".into();
        replacement.groups[g].shapes[s].pos = [80.0, 90.0, 100.0];
        app.apply_file_action(PendingFileAction::New(replacement.clone()));
        assert!(app.sk.live.is_none() && app.sk.edit.is_none() && !app.sk.on);
        assert!(app.drag.is_none() && app.multi.is_empty());
        app.live_discard();
        assert_eq!(app.scene, replacement, "Esc must not remove a body in the replacement document");
    }

    #[test]
    fn save_commits_a_valid_sketch_draft_and_rejects_an_incomplete_one() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let path = std::env::temp_dir().join(format!("meltcad-sketch-save-{}.meltcad", std::process::id()));
        app.path = Some(path.clone());
        app.set_sketch_tool(true);
        app.sk.pts = vec![[0.0, 0.0], [10.0, 0.0]];
        assert!(app.has_unsaved_work());
        assert!(!app.try_save(&ctx, false));
        app.sk.clear_drawing();
        app.sk.frame = crate::sketch::Frame::floor();
        app.sketch_done(crate::sketch_ui::Finished::Profile(crate::sketch::rect([0.0, 0.0], [20.0, 10.0])));
        assert!(app.try_save(&ctx, false));
        finish_file_jobs(&mut app, &ctx);
        assert!(!app.sk.on);
        let (saved, _) = crate::recovery::read_project(&path).expect("saved file");
        assert_eq!(saved.groups.iter().flat_map(|g| &g.shapes).filter(|s| s.guide).count(), 1);
        assert!(std::fs::remove_file(path).is_ok());
    }

    #[test]
    fn save_refuses_an_incomplete_solid_and_confirms_a_valid_preview() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        let path = std::env::temp_dir().join(format!("meltcad-solid-save-{}.meltcad", std::process::id()));
        app.path = Some(path.clone());
        app.set_sketch_tool(true);
        app.sk.frame = crate::sketch::Frame::floor();
        app.sketch_done(crate::sketch_ui::Finished::Profile(crate::sketch::rect([0.0, 0.0], [20.0, 10.0])));
        assert!(app.sketch_confirm(&ctx));
        assert!(app.make_from_selected_line(&ctx, crate::sketch_ui::Make::Sweep));
        assert!(!app.try_save(&ctx, false), "a missing path is never silently saved as a solid");
        app.set_sketch_tool(false);
        assert!(app.make_from_selected_line(&ctx, crate::sketch_ui::Make::Extrude));
        app.sk.height = 32.0;
        assert!(app.try_save(&ctx, false));
        finish_file_jobs(&mut app, &ctx);
        assert!(!app.sk.on && app.sk.live.is_none());
        let (saved, _) = crate::recovery::read_project(&path).expect("saved file");
        assert_eq!(saved.shape_count(), 2);
        assert_eq!(saved.groups[0].shapes[1].kind, Kind::Extrude);
        assert_eq!(saved.groups[0].shapes[1].size[0], 32.0);
        assert!(std::fs::remove_file(path).is_ok());
    }

    #[test]
    fn a_solid_draft_without_a_live_body_is_protected_by_file_actions() {
        use crate::sketch_ui::{Finished, Make, Stage};

        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        app.set_sketch_tool(true);
        app.sketch_done(Finished::Profile(crate::sketch::rect([-5.0, -5.0], [5.0, 5.0])));
        assert!(app.sketch_confirm(&ctx));
        let source = app.sel;
        app.set_sketch_tool(true);
        app.sketch_done(Finished::Line(vec![[0.0, 0.0], [0.0, 30.0], [20.0, 50.0]]));
        assert!(app.sketch_confirm(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("saved path") };
        app.settle(&ctx);
        app.committed = app.scene.clone();
        app.dirty = false;
        assert!(!app.has_unsaved_work(), "the saved source document starts clean");

        app.sel = source;
        assert!(app.make_from_selected_line(&ctx, Make::Sweep));
        assert!(app.use_secondary_sketch(g, s));
        assert_eq!(app.sk.stage, Stage::Ready);
        assert!(app.sk.live.is_none(), "this valid operation still has no scene body");
        assert_eq!(app.scene, app.committed);
        let path = app.sk.path.clone();
        for action in [PendingFileAction::New(Scene::default()), PendingFileAction::Open(PathBuf::from("another.meltcad")), PendingFileAction::Close]
        {
            app.request_file_action(action);
            assert!(app.pending_file_action.is_some(), "all document changes must offer Save/Discard/Cancel");
            assert_eq!(app.scene, app.committed);
            assert_eq!(app.sk.path, path);
            app.resolve_pending_file_action(&ctx, PendingFileDecision::Cancel);
            assert!(app.pending_file_action.is_none());
        }
    }

    #[test]
    fn a_linked_copy_rejects_add_shape_instead_of_losing_it_at_reload() {
        use crate::model::{Kind, Pick, Scene};
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Group(0));
        app.make_instance(&ctx);
        let Some(Pick::Group(link)) = app.sel else { panic!("linked copy selected") };
        assert!(app.scene.groups[link].is_linked());
        let count = app.scene.shape_count();
        app.add_shape(&ctx, Kind::Box);
        assert_eq!(app.scene.shape_count(), count, "a linked group cannot own shapes");
        assert!(app.scene.groups[link].shapes.is_empty());
        let reopened = Scene::from_json(&app.scene.to_json()).unwrap();
        assert_eq!(reopened.shape_count(), count);
        assert_eq!(reopened.groups[link].instance_of, app.scene.groups[link].instance_of);
    }

    #[test]
    fn curve_mirror_reflects_path_and_bezier_handle_geometry() {
        use crate::model::{CurveHandles, CurveNodeMode, Kind, Material, Pick, Shape};
        let mut app = App::new(true, None);
        let mut curve = Shape::new(Kind::Curve, [40.0, 0.0, 10.0], Material::default());
        curve.path = vec![[5.0, 0.0, 0.0, 1.0], [15.0, 0.0, 0.0, 1.0], [15.0, 10.0, 0.0, 1.0]];
        curve.params = [0.0, 0.0, 0.0, 1.0];
        curve.blend = 0.0;
        curve.curve_handles = vec![CurveHandles { mode: CurveNodeMode::Corner, incoming: [2.0, 3.0, 0.0], outgoing: [4.0, 5.0, 0.0] }];
        app.scene.groups[0].shapes.clear();
        app.scene.groups[0].shapes.push(curve.clone());
        app.sel = Some(Pick::Shape(0, 0));
        app.mirror_copy();
        let mirrored = app.scene.shape(0, 1).unwrap();
        assert_eq!(mirrored.pos[0], -40.0);
        assert_eq!(mirrored.path[0][0], -5.0);
        assert_eq!(mirrored.path[1][0], -15.0);
        assert_eq!(mirrored.curve_handles[0].incoming[0], -2.0);
        assert_eq!(mirrored.curve_handles[0].outgoing[0], -4.0);
        let scene_of = |sh: Shape| {
            let mut g = crate::model::Group::new("curve");
            g.shapes.push(sh);
            crate::model::Scene { groups: vec![g], ..Default::default() }
        };
        let a = crate::sdf::Field::new(&scene_of(curve));
        let b = crate::sdf::Field::new(&scene_of(mirrored.clone()));
        for p in [[55.0, 5.0, 10.0], [45.0, 0.0, 10.0], [50.0, 9.0, 10.0]] {
            let source = a.distance(p);
            let reflected = b.distance([-p[0], p[1], p[2]]);
            assert!((source - reflected).abs() < 0.01, "{p:?}: {source} != {reflected}");
        }
    }

    #[test]
    fn a_duplicate_stays_exactly_where_the_original_is() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        let at = app.scene.groups[0].shapes[0].pos;
        app.duplicate();
        assert_eq!(app.scene.groups[0].shapes[1].pos, at);
        app.sel = Some(Pick::Group(0));
        app.duplicate();
        assert_eq!(app.scene.groups[1].shapes[0].pos, at);
    }

    #[test]
    fn a_chosen_ball_moves_alone_and_a_group_still_moves_every_child() {
        use crate::model::{Group, Kind, Material, Pick, Shape};
        let mut app = super::App::new(true, None);
        app.scene.groups.clear();
        let mut group = Group::new("Pipe");
        let mut sibling = Shape::new(Kind::Sphere, [0.0, 0.0, 80.0], Material::default());
        sibling.name = "Sibling".into();
        group.shapes.push(sibling);
        let mut pipe = Shape::new(Kind::Curve, [10.0, 0.0, 10.0], Material::default());
        pipe.name = "Pipe".into();
        pipe.path = vec![[-20.0, 0.0, 0.0, 4.0], [0.0, 0.0, 0.0, 4.0], [20.0, 0.0, 0.0, 4.0], [40.0, 0.0, 0.0, 4.0], [60.0, 0.0, 0.0, 4.0]];
        group.shapes.push(pipe);
        app.scene.groups.push(group);
        app.committed_sync();

        // The row of "Ball 4" picks shape 1 and node 3.
        app.sel = Some(Pick::Shape(0, 1));
        app.ct.node = Some((0, 1, 3));
        assert_eq!(app.picks(), vec![Pick::Shape(0, 1)]);
        assert_eq!(app.sel_targets(), vec![(0, 1)], "the selection is the pipe shape, not the group");

        let before = app.scene.groups[0].shapes[1].path.clone();
        let pipe_pos = app.scene.groups[0].shapes[1].pos;
        let sibling_pos = app.scene.groups[0].shapes[0].pos;
        app.nudge([5.0, 0.0, 0.0]);
        let after = &app.scene.groups[0].shapes[1].path;
        for i in 0..before.len() {
            if i == 3 {
                assert!((after[i][0] - before[i][0] - 5.0).abs() < 1e-4, "the chosen ball moved by 5 mm");
                assert_eq!([after[i][1], after[i][2], after[i][3]], [before[i][1], before[i][2], before[i][3]]);
            } else {
                assert_eq!(after[i], before[i], "node {i} is untouched");
            }
        }
        assert_eq!(app.scene.groups[0].shapes[1].pos, pipe_pos, "the pipe itself did not move");
        assert_eq!(app.scene.groups[0].shapes[0].pos, sibling_pos, "its sibling shape did not move");

        // One nudge is one undo step.
        let ctx = egui::Context::default();
        app.settle(&ctx);
        assert_eq!(app.undo_depth(), 1);
        app.undo();
        assert_eq!(app.scene.groups[0].shapes[1].path, before, "undo puts the ball back");

        // The same movement survives save and reload.
        app.sel = Some(Pick::Shape(0, 1));
        app.ct.node = Some((0, 1, 3));
        app.nudge([1.0, 2.0, 3.0]);
        let moved = app.scene.groups[0].shapes[1].path.clone();
        let reloaded = crate::model::Scene::from_json(&app.scene.to_json()).unwrap_or_default();
        assert_eq!(reloaded.groups[0].shapes[1].path, moved, "the ball's offset round-trips");

        // A group pick still moves the pipe and every other shape in it (#260).
        app.sel = Some(Pick::Group(0));
        let sibling_x = app.scene.groups[0].shapes[0].pos[0];
        let pipe_x = app.scene.groups[0].shapes[1].pos[0];
        app.nudge([5.0, 0.0, 0.0]);
        assert_eq!(app.scene.groups[0].shapes[0].pos[0], sibling_x + 5.0);
        assert_eq!(app.scene.groups[0].shapes[1].pos[0], pipe_x + 5.0);
    }
}

/// The longest title that can be typed.
const MAX_TITLE_CHARS: usize = 80;

/// A typed title made safe to use as a file name: no path characters, trimmed, not too long.
fn clean_title(typed: &str) -> String {
    let kept: String = typed.chars().filter(|c| !c.is_control() && !"\\/:*?\"<>|".contains(*c)).collect();
    kept.trim().trim_matches('.').trim().chars().take(MAX_TITLE_CHARS).collect()
}

#[cfg(test)]
mod title_tests {
    use super::clean_title;

    #[test]
    fn new_does_not_keep_the_last_documents_title() {
        let mut app = super::App::new(true, None);
        app.doc_title = "Foo".into();
        app.title_edit = Some("Fo".into());
        app.apply_file_action(super::PendingFileAction::New(crate::model::Scene::default()));
        assert_eq!(app.doc_name(), "Untitled");
        assert!(app.title_edit.is_none());
    }

    #[test]
    fn opening_a_document_does_not_auto_overwrite_its_animation() {
        use crate::model::Scene;
        let ctx = egui::Context::default();
        let mut app = super::App::new(true, None);
        app.an.open = true;
        app.an.frame = 10;
        app.an.playing = true;

        let mut imported = Scene::default();
        let mut animation = std::mem::take(&mut imported.anim);
        imported.groups[0].shapes[0].pos[0] = 0.0;
        animation.capture(&mut imported, 0).unwrap();
        imported.groups[0].shapes[0].pos[0] = 100.0;
        animation.capture(&mut imported, 10).unwrap();
        imported.anim = animation;
        let key_before = imported.anim.tracks[0].keys[0].1.pos;

        app.apply_file_action(super::PendingFileAction::New(imported));
        assert!(!app.an.playing);
        assert_eq!(app.an.frame, 0);
        app.animation_capture_finished(&ctx);
        assert_eq!(app.scene.anim.tracks[0].keys[0].1.pos, key_before);
        assert!(!app.dirty, "loading must not dirty or change the document");
    }

    #[test]
    fn renaming_waits_for_a_running_file_job() {
        let ctx = egui::Context::default();
        let mut app = super::App::new(true, None);
        let dir = std::env::temp_dir().join(format!("melt3d-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("Old.meltcad");
        std::fs::write(&old, b"saved design").unwrap();
        app.path = Some(old.clone());
        app.queued_open = Some(super::QueuedOpen { path: dir.join("Other.meltcad"), base_revision: 0 });
        app.rename_doc(&ctx, "New");
        assert_eq!(app.path.as_deref(), Some(old.as_path()), "a pending file job keeps the old path");
        assert!(old.exists(), "and the file is not moved under it");
        app.queued_open = None;
        app.rename_doc(&ctx, "New");
        let stem = app.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
        assert_eq!(stem.as_deref(), Some("New"), "without a job the rename happens");
        assert!(!old.exists() && app.path.as_ref().is_some_and(|p| p.exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arrow_keys_move_nested_groups_exactly_once() {
        use crate::model::{Group, Kind, Material, Pick, Shape};
        let mut app = super::App::new(true, None);
        let mut child = Group::new("child");
        child.depth = 1;
        child.shapes.push(Shape::new(Kind::Box, [40.0, 0.0, 10.0], Material::default()));
        app.scene.groups.push(child);
        let uid = app.scene.ensure_group_uid(0).unwrap();
        let mut linked = Group::new("nested linked copy");
        linked.depth = 2;
        linked.instance_of = Some(uid);
        app.scene.groups.push(linked);
        app.sel = Some(Pick::Group(0));
        app.nudge([10.0, 0.0, 0.0]);
        assert_eq!(app.scene.groups[0].shapes[0].pos[0], 10.0);
        assert_eq!(app.scene.groups[1].shapes[0].pos[0], 50.0);
        assert_eq!(app.scene.groups[2].xform.pos[0], 0.0, "the copy follows its moving source, not twice");
        // The same child also selected on its own must not move twice.
        app.multi_for = app.sel;
        app.multi.push(Pick::Shape(1, 0));
        app.nudge([10.0, 0.0, 0.0]);
        assert_eq!(app.scene.groups[1].shapes[0].pos[0], 60.0);
        assert_eq!(app.scene.groups[2].xform.pos[0], 0.0);
    }

    #[test]
    fn moving_a_parent_with_a_linked_copy_does_not_double_move_the_copy() {
        use crate::model::{Group, Kind, Material, Pick, Shape};
        let mut app = super::App::new(true, None);
        app.scene.groups.clear();
        app.scene.groups.push(Group::new("Parent"));
        let mut source = Group::new("Source");
        source.depth = 1;
        source.shapes.push(Shape::new(Kind::Box, [0.0, 0.0, 10.0], Material::default()));
        app.scene.groups.push(source);
        let uid = app.scene.ensure_group_uid(1).expect("source uid");
        let mut link = Group::new("Copy");
        link.depth = 1;
        link.instance_of = Some(uid);
        link.xform.pos = [30.0, 0.0, 0.0];
        app.scene.groups.push(link);
        // The copy stands 30 mm to the right of the source; moving the parent 10 mm must take it
        // 10 mm, not 20 (#261).
        app.sel = Some(Pick::Group(0));
        app.nudge([10.0, 0.0, 0.0]);
        assert_eq!(app.scene.groups[1].shapes[0].pos[0], 10.0);
        assert_eq!(app.scene.groups[2].xform.pos[0], 30.0, "the copy keeps its offset and follows once");
    }

    #[test]
    fn arrow_keys_move_a_linked_copy_through_its_transform() {
        use crate::model::{Group, Pick};
        let mut app = super::App::new(true, None);
        let uid = app.scene.ensure_group_uid(0).unwrap();
        let mut link = Group::new("link");
        link.instance_of = Some(uid);
        app.scene.groups.push(link);
        let at = app.scene.groups.len() - 1;
        app.sel = Some(Pick::Group(at));
        app.nudge([5.0, 0.0, 2.0]);
        assert_eq!(app.scene.groups[at].xform.pos, [5.0, 0.0, 2.0]);
        assert_eq!(app.scene.groups[0].shapes[0].pos, super::App::new(true, None).scene.groups[0].shapes[0].pos, "the source stays");
    }

    #[test]
    fn titles_are_made_safe_for_file_names() {
        assert_eq!(clean_title("  My vase  "), "My vase");
        assert_eq!(clean_title("a/b\\c:d"), "abcd");
        assert_eq!(clean_title("..."), "");
        assert_eq!(clean_title("\n\t"), "");
        assert_eq!(clean_title(&"x".repeat(500)).chars().count(), 80);
    }

    fn picture_dir(name: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("meltcad_picture_{name}_{stamp}"));
        std::fs::create_dir_all(&dir).expect("picture directory");
        dir
    }

    fn wait_for_picture(app: &mut super::App, ctx: &egui::Context) {
        for _ in 0..1200 {
            app.poll_render_job(ctx);
            if app.render_job.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("control picture worker did not finish");
    }

    #[test]
    fn a_control_picture_is_drawn_on_a_worker_while_the_frame_loop_keeps_running() {
        let dir = picture_dir("worker");
        let path = dir.join("sheet.png");
        let release = crate::render_job::test_gate::hold(&path);
        let ctx = egui::Context::default();
        let mut app = super::App::new(true, None);
        let (tx, rx) = std::sync::mpsc::channel();
        let (reply, answer) = std::sync::mpsc::channel();
        app.control = Some(rx);
        tx.send(crate::control::Request { cmd: serde_json::json!({"cmd": "render", "path": path.display().to_string(), "size": 64}), reply })
            .expect("queue the request");

        app.run_control(&ctx);
        assert!(app.render_job.is_some(), "the request returned while the picture is still being made");
        assert!(
            matches!(answer.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
            "the window thread must not answer before a complete picture exists"
        );

        // The held worker cannot draw unless another thread runs: a whole frame still advances.
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 600.0))), ..Default::default() };
        let mut drew = 0;
        let mut output = ctx.run_ui(input, |ui| {
            drew += 1;
            app.frame_body(ui);
        });
        output.textures_delta.clear();
        assert_eq!(drew, 1, "a frame was drawn");
        assert!(app.render_job.is_some(), "the frame finished while the picture was still being drawn");
        assert!(!path.exists(), "nothing is written before the picture is complete");
        assert!(matches!(answer.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)), "still no answer");

        release.send(()).expect("release the worker");
        wait_for_picture(&mut app, &ctx);
        let answer = answer.recv_timeout(std::time::Duration::from_secs(10)).expect("the request was answered").expect("picture saved");
        assert_eq!(answer["png"], serde_json::json!(path.display().to_string()));
        let bytes = std::fs::read(&path).expect("picture file");
        assert!(bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]), "a complete PNG is on disk");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1, "no staged file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stopped_control_picture_keeps_the_previous_file_and_leaves_no_partial_output() {
        let dir = picture_dir("stopped");
        let path = dir.join("sheet.png");
        std::fs::write(&path, b"previous picture").expect("previous output");
        let release = crate::render_job::test_gate::hold(&path);
        let ctx = egui::Context::default();
        let mut app = super::App::new(true, None);
        let (tx, rx) = std::sync::mpsc::channel();
        let (reply, answer) = std::sync::mpsc::channel();
        app.control = Some(rx);
        tx.send(crate::control::Request { cmd: serde_json::json!({"cmd": "render", "path": path.display().to_string(), "size": 40}), reply })
            .expect("queue the request");
        app.run_control(&ctx);
        app.render_job.as_ref().expect("worker started").cancel();
        release.send(()).expect("release the worker");
        wait_for_picture(&mut app, &ctx);

        let result = answer.recv_timeout(std::time::Duration::from_secs(10)).expect("the request was answered");
        assert!(result.is_err(), "a stopped picture is reported as an error");
        assert_eq!(std::fs::read(&path).expect("old output"), b"previous picture", "the previous picture is untouched");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 1, "no staged partial file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_control_picture_writes_nothing() {
        let dir = picture_dir("failed");
        let path = dir.join("missing").join("sheet.png");
        let ctx = egui::Context::default();
        let mut app = super::App::new(true, None);
        let (tx, rx) = std::sync::mpsc::channel();
        let (reply, answer) = std::sync::mpsc::channel();
        app.control = Some(rx);
        tx.send(crate::control::Request { cmd: serde_json::json!({"cmd": "render", "path": path.display().to_string(), "size": 40}), reply })
            .expect("queue the request");
        app.run_control(&ctx);
        wait_for_picture(&mut app, &ctx);

        let result = answer.recv_timeout(std::time::Duration::from_secs(10)).expect("the request was answered");
        assert!(result.is_err(), "a write failure is reported");
        assert!(!path.exists(), "a failed picture is not published");
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 0, "no temporary output is left");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
