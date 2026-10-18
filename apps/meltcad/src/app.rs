//! The window: top bar, shape palette, viewport, side panels, shortcuts, undo and files.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{Color32, RichText, Ui};

use crate::camera::Camera;
use crate::gizmo::{Drag, Tool};
use crate::model::{Group, Kind, MAX_GROUPS, MAX_SHAPES, Material, Op, Pick, Scene, Shape};
use crate::sdf::Field;
use crate::view::centre_of;
use crate::{glyphs, style};

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

enum PendingFileAction {
    New(Scene),
    Open(PathBuf),
    Close,
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
    /// More picks next to `sel` (Shift + click). Only valid while `multi_for` is still `sel`.
    pub multi: Vec<Pick>,
    multi_for: Option<Pick>,
    pub tool: Tool,
    /// What a new shape does: add to the group or carve from it.
    pub new_op: Op,
    pub drag: Option<Drag>,
    pub srgb: bool,
    pub last_view_height: f32,
    /// The scene as of the last `track_scene` and its revision number: a change is found once per
    /// frame by comparing, and everything else (the picture, the field, the undo step) asks the
    /// number instead of comparing whole scenes again.
    tracked: Scene,
    scene_rev: u64,
    /// The revision `committed` is current for, and the one `dedupe_uids` last ran for.
    committed_rev: u64,
    deduped_rev: u64,
    /// The field for picking, made when picking needs it and the scene has changed since.
    field_cache: std::cell::RefCell<Option<(u64, std::rc::Rc<Field>)>>,
    /// How many times the field was built (tests and the HUD look at it).
    pub(crate) field_builds: std::cell::Cell<u64>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    recovery: crate::recovery::Store,
    pending_file_action: Option<PendingFileAction>,
    close_approved: bool,
    status: (String, f64),
    pub renaming: Option<Pick>,
    pub folded: HashSet<usize>,
    /// A row of the scene list being dragged, and where every row is on screen this frame.
    pub layer_drag: Option<Pick>,
    pub row_rects: Vec<(Pick, egui::Rect)>,
    shot: Option<PathBuf>,
    /// The viewport's place on screen (for a picture of it).
    pub view_rect: egui::Rect,
    /// A picture being taken: the overlays are hidden until it arrives.
    pub capture: Option<Capture>,
    /// The shape under the mouse.
    pub hover: Option<Pick>,
    /// Where the camera is gliding to.
    pub fly: Option<Camera>,
    /// A left-drag that began on empty space is turning the view.
    pub orbiting: bool,
    pub show_keys: bool,
    /// Which tab of the right panel is showing.
    pub tab: Tab,
    /// The material list's chosen category (None = the person's saved ones).
    pub mat_cat: Option<crate::materials::Category>,
    /// A material picked from the list goes to the whole group.
    pub mat_group: bool,
    /// The curve tool (nodes, drawing).
    pub ct: crate::curve_tool::State,
    /// The pipe (trace) tool.
    pub pt: crate::pipe_tool::State,
    /// The sketch tool (plane, pen, finished profile) and the outline editing.
    pub sk: crate::sketch_ui::State,
    /// Frame cache, fast mode, quality and other speed-ups (see `perf.rs`).
    pub perf: crate::perf::Perf,
    /// The last copy (text), so the Edit menu can paste without asking the system.
    pub clipboard: Option<String>,
    /// The library card (ready-made models): open or not, which category, the search text.
    pub lib_open: bool,
    pub lib_cat: usize,
    pub lib_query: String,
    /// A message from code that has no window context (shown in the status bar next frame).
    pub notice: Option<String>,
    /// The graphics path (GPU picture or the simple CPU one) and its cards.
    pub gpu: crate::gpu_ui::GpuUi,
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
            last_view_height: 600.0,
            path: None,
            dirty: false,
            recovery: crate::recovery::Store::new(crate::recovery::default_directory(), std::time::Instant::now()),
            pending_file_action: None,
            close_approved: false,
            status: (String::new(), 0.0),
            renaming: None,
            folded: HashSet::new(),
            layer_drag: None,
            row_rects: Vec::new(),
            shot: std::env::var_os("MELTCAD_TEST_DIR").map(PathBuf::from),
            view_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            capture: None,
            hover: None,
            fly: None,
            orbiting: false,
            show_keys: false,
            tab: Tab::Shape,
            mat_cat: Some(crate::materials::Category::Basic),
            mat_group: false,
            ct: Default::default(),
            pt: Default::default(),
            sk: Default::default(),
            perf: crate::perf::Perf {
                sharpen: crate::perf::DEFAULT_SHARPEN,
                hud: std::env::var_os("MELTCAD_HUD").is_some_and(|v| v != "0"),
                ..Default::default()
            },
            clipboard: None,
            lib_open: false,
            lib_cat: 0,
            lib_query: String::new(),
            notice: None,
            gpu: Default::default(),
            bench: Default::default(),
        };
        if let Some(p) = open {
            app.open_path(p);
        }
        app
    }

    /// The field for picking. It is built when asked for, and again only after the scene changed
    /// (a drag or a camera move that picks nothing never builds it).
    pub fn field(&self) -> std::rc::Rc<Field> {
        let mut cache = self.field_cache.borrow_mut();
        if let Some((rev, f)) = cache.as_ref()
            && *rev == self.scene_rev
        {
            return f.clone();
        }
        self.field_builds.set(self.field_builds.get() + 1);
        let f = std::rc::Rc::new(Field::new(&self.scene));
        *cache = Some((self.scene_rev, f.clone()));
        f
    }

    /// Notice a change of the scene: compares with the copy kept from the last look, and when it
    /// differs bumps the revision number. Returns the number.
    pub(crate) fn track_scene(&mut self) -> u64 {
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
            ui.heading("Unsaved work found");
            ui.label(format!("Recovery data: {}", path.display()));
            ui.label("Restoring will not overwrite the original file.");
            ui.horizontal(|ui| {
                if ui.button("Restore as unsaved copy").clicked() {
                    choice = Some(0);
                }
                if ui.button("Later").clicked() {
                    choice = Some(1);
                }
                if ui.button("Delete recovery data").clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => match self.recovery.recover_pending() {
                Ok((scene, notes, _source)) => {
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
                    self.say(ctx, if notes.is_empty() { "Restored as unsaved copy".into() } else { format!("Restored: {}", notes.join(", ")) });
                }
                Err(e) => {
                    self.say(ctx, format!("Recovery file is damaged or cannot be opened: {e}"));
                    self.recovery.postpone();
                }
            },
            Some(1) => self.recovery.postpone(),
            Some(2) => {
                if let Err(e) = self.recovery.discard_pending() {
                    self.say(ctx, format!("Could not delete recovery data: {e}"));
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

    fn fix_selection(&mut self) {
        let ok = match self.sel {
            Some(Pick::Shape(g, s)) => self.scene.shape(g, s).is_some(),
            Some(Pick::Group(g)) => g < self.scene.groups.len(),
            None => true,
        };
        if !ok {
            self.sel = None;
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.scene, prev.clone()));
            self.committed = prev;
            self.fix_selection();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.scene, next.clone()));
            self.committed = next;
            self.fix_selection();
        }
    }

    pub fn new_scene(&mut self, scene: Scene) {
        self.request_file_action(PendingFileAction::New(scene));
    }

    fn has_unsaved_work(&self) -> bool {
        self.dirty || self.scene != self.committed
    }

    fn request_file_action(&mut self, action: PendingFileAction) {
        if self.pending_file_action.is_some() {
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
        let (scene, path, notes) = match action {
            PendingFileAction::New(scene) => (scene, None, Vec::new()),
            PendingFileAction::Open(path) => match crate::recovery::read_project(&path) {
                Ok((scene, notes)) => (scene, Some(path), notes),
                Err(e) => {
                    self.status = (format!("Could not open: {e}"), 0.0);
                    return;
                }
            },
            PendingFileAction::Close => return,
        };
        if let Err(e) = self.recovery.clear_active() {
            self.notice = Some(format!("Could not clean up recovery data: {e}"));
        }
        self.scene = scene.clone();
        self.committed = scene;
        self.undo.clear();
        self.redo.clear();
        self.sel = if self.scene.shape(0, 0).is_some() { Some(Pick::Shape(0, 0)) } else { None };
        self.path = path;
        self.dirty = false;
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
                    self.finish_pending_file_action(ctx, action);
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
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved && (self.has_unsaved_work() || self.pending_file_action.is_some()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending_file_action.is_none() {
                self.pending_file_action = Some(PendingFileAction::Close);
            }
            self.capture = None;
        }
    }

    fn pending_file_dialog(&mut self, ctx: &egui::Context) {
        if self.pending_file_action.is_none() {
            return;
        }
        let mut decision = None;
        egui::Modal::new(egui::Id::new("unsaved_file_action")).show(ctx, |ui| {
            ui.heading("You have unsaved changes");
            ui.label("Save current work before continuing?");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save and continue").clicked() {
                    decision = Some(PendingFileDecision::Save);
                }
                if ui.button("Continue without saving").clicked() {
                    decision = Some(PendingFileDecision::Discard);
                }
                if ui.button("Cancel").clicked() {
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
            Some(g) if g < self.scene.groups.len() => Some(g),
            _ => {
                self.scene.groups.push(Group::new("Group 1"));
                Some(self.scene.groups.len() - 1)
            }
        }
    }

    pub fn add_shape(&mut self, ctx: &egui::Context, kind: Kind) {
        if self.scene.shape_count() >= MAX_SHAPES {
            self.say(ctx, format!("Up to {MAX_SHAPES} shapes"));
            return;
        }
        let Some(g) = self.target_group() else { return };
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
        if self.scene.groups.len() < MAX_GROUPS {
            let n = self.scene.groups.len() + 1;
            self.scene.groups.push(Group::new(format!("Group {n}")));
            self.sel = Some(Pick::Group(self.scene.groups.len() - 1));
        }
    }

    pub fn duplicate(&mut self) {
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape_count() < MAX_SHAPES => {
                if let Some(mut c) = self.scene.shape(g, s).cloned() {
                    if let Err(why) = crate::mesh::can_add(&self.scene, std::iter::once(&c)) {
                        self.notice = Some(why.into());
                        return;
                    }
                    c.pos = [c.pos[0] + 8.0, c.pos[1] + 8.0, c.pos[2]];
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
                for (i, mut c) in copies.into_iter().enumerate() {
                    if i == 0 {
                        c.name = format!("{} copy", c.name);
                    }
                    for s in &mut c.shapes {
                        s.pos = [s.pos[0] + 8.0, s.pos[1] + 8.0, s.pos[2]];
                    }
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
            c.name = format!("{} (mirror)", c.name);
            self.scene.groups[g].shapes.insert(s + 1, c);
            self.sel = Some(Pick::Shape(g, s + 1));
        }
    }

    /// Everything that is selected: `sel` first, then the Shift + click ones.
    pub fn picks(&self) -> Vec<Pick> {
        let mut v: Vec<Pick> = self.sel.into_iter().collect();
        if self.sel.is_some() && self.multi_for == self.sel {
            v.extend(self.multi.iter().copied().filter(|p| Some(*p) != self.sel));
        }
        v
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
        let mut shapes: Vec<(usize, usize)> =
            self.picks().into_iter().filter_map(|p| if let Pick::Shape(g, s) = p { Some((g, s)) } else { None }).collect();
        if shapes.len() < 2 || self.scene.groups.len() >= MAX_GROUPS {
            return;
        }
        shapes.sort();
        shapes.dedup();
        let mut group = Group::new(format!("Group {}", self.scene.groups.len() + 1));
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

    pub fn delete_selected(&mut self) {
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
                let sh = self.scene.groups[g].shapes.remove(s);
                self.scene.groups[g2].shapes.push(sh);
                self.sel = Some(Pick::Shape(g2, self.scene.groups[g2].shapes.len() - 1));
            }
            (Pick::Group(g), Pick::Group(g2) | Pick::Shape(g2, _)) => {
                if g >= self.scene.groups.len() || g2 >= self.scene.groups.len() || g == g2 {
                    return;
                }
                let e = self.scene.subtree_end(g);
                if (g..e).contains(&g2) {
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

    /// Move the selection by `d` millimetres.
    pub fn nudge(&mut self, d: [f32; 3]) {
        for (g, s) in self.sel_targets() {
            if let Some(sh) = self.scene.shape_mut(g, s) {
                sh.pos = [sh.pos[0] + d[0], sh.pos[1] + d[1], sh.pos[2] + d[2]];
            }
        }
    }

    /// Switch plus and minus on the selection.
    pub fn flip_op(&mut self) {
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
        if let Some(p) = rfd::FileDialog::new().add_filter("MELTCAD", &["meltcad", "yura", "json"]).pick_file() {
            self.open_path(p);
        }
    }

    fn save(&mut self, ctx: &egui::Context, ask: bool) {
        self.try_save(ctx, ask);
    }

    fn try_save(&mut self, ctx: &egui::Context, ask: bool) -> bool {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("MELTCAD", &["meltcad"]).set_file_name("Untitled.meltcad").save_file(),
        };
        let Some(path) = path else { return false };
        match crate::recovery::save_project(&path, &self.scene) {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                let revision = self.track_scene();
                if let Err(e) = self.recovery.document_saved(revision) {
                    self.notice = Some(format!("Could not clean up recovery data: {e}"));
                }
                self.say(ctx, "Saved");
                true
            }
            Err(e) => {
                self.say(ctx, format!("Could not save: {e}"));
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
        crate::export_dialog::open(ctx, Some(format));
    }

    /// Open the export card and measure the model for printing (size, volume, holes, thin walls, overhangs).
    pub fn print_check(&mut self, ctx: &egui::Context) {
        crate::export_dialog::open_check(ctx);
    }

    // ----- shortcuts -----

    pub(crate) fn keys(&mut self, ctx: &egui::Context) {
        if self.pending_file_action.is_some() || self.recovery.pending_path().is_some() {
            return;
        }
        // Dimensions have their own Tab/Enter/Esc lifecycle. Never dispatch text-input
        // keys as view shortcuts (in particular Tab, 1/3/5/7 or Delete).
        if self.sk.dimension.as_ref().is_some_and(|d| d.editing) {
            self.sketch_keys(ctx);
            return;
        }
        if ctx.egui_wants_keyboard_input() {
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
                    ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(30.0, 30.0 * 131.0 / 192.0)).tint(style::TEXT));
                    ui.label(RichText::new("Melt 3D").size(14.0).strong().color(style::TEXT));
                    style::vsep(ui);
                    style::menu_button(ui, "File", false, |ui| {
                        if ui.button("New").clicked() {
                            self.new_scene(Scene::default());
                            ui.close();
                        }
                        if ui.button("Open…  Ctrl+O").clicked() {
                            self.open_dialog(&ctx);
                            ui.close();
                        }
                        if ui.button("Import SVG…").clicked() {
                            crate::svg_dialog::open(&ctx);
                            ui.close();
                        }
                        if ui.button("Import mesh…").on_hover_text("STL, OBJ, 3MF, PLY, GLB").clicked() {
                            crate::mesh_dialog::open(&ctx);
                            ui.close();
                        }
                        if ui.button("Save  Ctrl+S").clicked() {
                            self.save(&ctx, false);
                            ui.close();
                        }
                        if ui.button("Save as…").clicked() {
                            self.save(&ctx, true);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Export…").on_hover_text("STL, 3MF, OBJ, PLY, GLB").clicked() {
                            crate::export_dialog::open(&ctx, None);
                            ui.close();
                        }
                        if ui.button("Export again with last settings  Ctrl+E").clicked() {
                            self.export_stl(&ctx);
                            ui.close();
                        }
                        if ui.button("Export image (PNG)").clicked() {
                            self.start_png(&ctx);
                            ui.close();
                        }
                    });
                    style::menu_button(ui, "Edit", false, |ui| {
                        if ui.button("Copy  Ctrl+C").clicked() {
                            self.copy(&ctx);
                            ui.close();
                        }
                        if ui.button("Cut  Ctrl+X").clicked() {
                            self.cut(&ctx);
                            ui.close();
                        }
                        if ui.button("Paste  Ctrl+V").clicked() {
                            self.paste(&ctx, None);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Duplicate  Ctrl+D").clicked() {
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
                    style::vsep(ui);
                    if style::icon_button(ui, 32.0, "Undo (Ctrl+Z)", false, glyphs::undo).clicked() {
                        self.undo();
                    }
                    if style::icon_button(ui, 32.0, "Redo (Ctrl+Shift+Z)", false, glyphs::redo).clicked() {
                        self.redo();
                    }
                    style::vsep(ui);
                    if style::pill(ui, "Find  F", false).on_hover_text("Find shapes, library, commands (F)").clicked() {
                        palette::open(&ctx);
                    }
                    let left_end = ui.cursor().left();
                    let right_start = ui.max_rect().right();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        style::menu_button(ui, "Export", true, |ui| {
                            if ui.button("Export…").on_hover_text("STL, 3MF, OBJ, PLY, GLB. Detail, units, color").clicked() {
                                crate::export_dialog::open(&ctx, None);
                                ui.close();
                            }
                            if ui.button("Export again").on_hover_text("Ctrl+E").clicked() {
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
                        let mut menu = crate::perf::MenuState { quality: self.perf.quality, hud: self.perf.hud, bench: false };
                        crate::perf::quality_menu(ui, &mut menu, |ui| self.gpu_menu_items(ui));
                        self.perf.quality = menu.quality;
                        self.perf.hud = menu.hud;
                        self.perf.bench_requested |= menu.bench;
                    });
                    // the file name in the middle of the bar
                    let name =
                        self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
                    let label = format!("{}{}", name, if self.dirty { " ●" } else { "" });
                    let cx = bar.center().x.clamp(left_end + 60.0, (right_start - 220.0).max(left_end + 60.0));
                    ui.painter().text(
                        egui::pos2(cx, bar.center().y),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(13.0),
                        style::DIM,
                    );
                });
            },
        );
    }

    /// The tools down the left side of the viewport: the shapes to add, the curve, the operation, groups.
    fn tool_strip(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("tool_strip")).order(egui::Order::Middle).fixed_pos(rect.left_top() + egui::vec2(12.0, 12.0)).show(
            &ctx,
            |ui| {
                style::strip().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
                    // a tall strip scrolls instead of running off a small window
                    egui::ScrollArea::vertical()
                        .max_height((rect.height().min(ctx.content_rect().bottom() - rect.top()) - 56.0).max(120.0))
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            ui.set_max_width(style::STRIP_W);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                for k in Kind::MAIN {
                                    let r = style::wide_button(ui, k.label(), k.label(), false, None, |p, r, ink| {
                                        glyphs::kind(p, r, k, ink, Color32::from_white_alpha(18))
                                    });
                                    if r.clicked() {
                                        self.add_shape(&ctx, k);
                                    }
                                }
                                self.curve_button(ui);
                                self.pipe_button(ui);
                                self.sketch_button(ui);
                                style::menu_button(ui, "…", false, |ui| {
                                    ui.set_min_width(190.0);
                                    ui.label(egui::RichText::new("Other shapes").size(12.0).color(style::DIM));
                                    for k in Kind::MORE {
                                        let r = ui.horizontal(|ui| {
                                            let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                            glyphs::kind(ui.painter(), rc, k, ui.visuals().text_color(), Color32::from_white_alpha(18));
                                            ui.selectable_label(false, k.label())
                                        });
                                        if r.inner.clicked() {
                                            self.add_shape(&ctx, k);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    ui.label(egui::RichText::new("CAD tools").size(12.0).color(style::DIM));
                                    for k in Kind::CAD {
                                        let r = ui.horizontal(|ui| {
                                            let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                            glyphs::kind(ui.painter(), rc, k, ui.visuals().text_color(), Color32::from_white_alpha(18));
                                            ui.selectable_label(false, k.label())
                                        });
                                        if r.inner.clicked() {
                                            self.add_shape(&ctx, k);
                                            ui.close();
                                        }
                                    }
                                });
                                ui.end_row();
                                style::hsep_w(ui, style::STRIP_W);
                                ui.end_row();
                                for op in [Op::Add, Op::Subtract, Op::Paint, Op::Intersect] {
                                    let on = self.new_op == op;
                                    let (name, tip) = match op {
                                        Op::Add => ("Add", "Join: the new shape is added to the model (Tab flips Add / Cut)"),
                                        Op::Subtract => ("Cut", "Cut: the new shape carves a hole out of the model"),
                                        Op::Paint => ("Paint", "Paint: the new shape only changes the colour where it touches"),
                                        Op::Intersect => ("Overlap", "Overlap: keep only the part where the new shape and the model meet"),
                                    };
                                    if style::wide_button(ui, name, tip, on, Some(style::op_color(op)), |p, r, ink| glyphs::op(p, r, op, ink))
                                        .clicked()
                                    {
                                        self.new_op = op;
                                    }
                                }
                                ui.end_row();
                                style::hsep_w(ui, style::STRIP_W);
                                ui.end_row();
                                if style::wide_button(ui, "New group", "New group", false, None, glyphs::group).clicked() {
                                    self.add_group();
                                }
                            });
                        });
                });
            },
        );
    }

    /// The docked panel on the left: the scene list (outliner) and the ready-made models (assets).
    fn left_dock(&mut self, ui: &mut Ui) {
        egui::Panel::left("dock")
            .exact_size(272.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 10)))
            .show(ui, |ui| {
                style::panel_sheen(ui, ui.max_rect().expand2(egui::vec2(10.0, 10.0)), style::PANEL_DARK, style::Edge::Left);
                let tabs = [("Scene", ""), ("Library", "")];
                if let Some(i) = style::segmented(ui, "dock_tabs", &tabs, Some(usize::from(self.lib_open)), true) {
                    self.lib_open = i == 1;
                }
                ui.add_space(6.0);
                if self.lib_open {
                    crate::library_ui::panel(self, ui);
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} shapes", self.scene.shape_count())).size(12.0).color(style::DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if style::icon_button(ui, 26.0, "New group", false, glyphs::group).clicked() {
                                self.add_group();
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
                    ui.separator();
                    for (k, v) in [
                        ("Middle drag", "Orbit"),
                        ("Right drag", "Pan"),
                        ("Wheel", "Zoom to cursor"),
                        ("Left drag (empty)", "Orbit"),
                        ("Click", "Select  Shift: add/remove  Ctrl: whole group"),
                        ("Double click", "Focus on shape"),
                        ("Drawing: left click → right click", "Place point → finish (all draw tools)"),
                        ("F", "Find shapes, library, commands"),
                        ("Shift+F / Home", "Frame selection / Frame all"),
                        ("1 3 7 (Ctrl: opposite)", "Front / Right / Top"),
                        ("5", "Perspective ⇄ Ortho"),
                        ("H / Alt+H", "Hide ⇄ show / Show all"),
                        ("Tab", "Add ⇄ Subtract"),
                        ("Ctrl + drag", "1 mm / 15° steps"),
                        ("Arrow keys", "Move 1 mm (Shift: 10 mm)"),
                        ("Ctrl+C / X / V", "Copy / Cut / Paste"),
                        ("Ctrl+D / Delete", "Duplicate / Delete"),
                        ("Ctrl+G", "Group the selected shapes"),
                        ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_sized([150.0, 18.0], egui::Label::new(RichText::new(k).size(12.0).color(style::ACCENT)));
                            ui.label(RichText::new(v).size(12.0).color(style::TEXT));
                        });
                    }
                });
            });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let now = ui.input(|i| i.time);
        egui::Panel::bottom("status")
            .exact_size(28.0)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(14, 3)))
            .show(ui, |ui| {
                style::panel_sheen(ui, ui.max_rect().expand2(egui::vec2(14.0, 3.0)), style::PANEL_DARK, style::Edge::Top);
                ui.horizontal_centered(|ui| {
                    let note = if now - self.status.1 < 5.0 { self.status.0.clone() } else { String::new() };
                    if !note.is_empty() {
                        // the message goes away by itself: wake up when it is due
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((5.0 - (now - self.status.1)).max(0.0) + 0.05));
                    }
                    let hint = crate::gizmo::hint(self.tool);
                    ui.label(RichText::new(if note.is_empty() { hint.to_string() } else { note }).size(12.0).color(style::DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Untitled".into());
                        let dims = self.selection_size().map(|d| format!(" | {d}")).unwrap_or_default();
                        ui.label(
                            RichText::new(format!("{}{} | Shapes {}{}", name, if self.dirty { " *" } else { "" }, self.scene.shape_count(), dims))
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
        match self.sel? {
            Pick::Shape(g, s) => {
                let sh = self.scene.shape(g, s)?;
                let h = sh.local_half();
                let turned = sh.rot.iter().any(|r| (r % 90.0).abs() > 0.01);
                if turned {
                    return Some(format!("About {:.0} mm", crate::view::radius_of(sh) * 2.0));
                }
                // a quarter turn about one axis swaps two sizes: keep it simple and exact only when not turned
                let dim = |i: usize| h[i] * sh.scale[i] * 2.0;
                Some(format!("W {:.1} × D {:.1} × H {:.1} mm", dim(0), dim(1), dim(2)))
            }
            Pick::Group(g) => {
                let (_, r) = crate::gpu::bounds(&self.scene, Some(Pick::Group(g)));
                (r > 0.0).then(|| format!("Group about {:.0} mm", r * 2.0))
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
    fn on_exit(&mut self) {
        if let Err(e) = self.recovery.normal_exit() {
            crate::gpu_log::log(&format!("recovery cleanup failed: {e}"));
        }
    }
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame_body(ui);
    }
}

impl App {
    /// One frame of the window (what `eframe::App::ui` does; a method of its own so that tests can run it).
    pub(crate) fn frame_body(&mut self, ui: &mut Ui) {
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first ui frame");
        let ctx = ui.ctx().clone();
        let frame_began = std::time::Instant::now();
        self.perf.stats.begin(ctx.input(|i| i.time));
        self.fix_selection();
        self.handle_close_request(&ctx);
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
            self.keys_card(ui);
            crate::svg_dialog::update(self, &ctx);
            crate::mesh_dialog::update(self, &ctx);
            crate::array::update(self, &ctx);
            crate::export_dialog::update(self, &ctx);
            self.palette_ui(&ctx);
            if std::mem::take(&mut self.perf.bench_requested) {
                let backends = self.current_backend().map(|b| b.to_wgpu());
                self.bench.start(&ctx, backends);
            }
            self.bench.show(&ctx);
        }
        self.pending_file_dialog(&ctx);
        self.recovery_dialog(&ctx);
        self.capture_step(&ctx);
        self.settle(&ctx);
        if !cfg!(test) {
            ctx.request_repaint_after(crate::recovery::INTERVAL);
            if self.recovery.pending_path().is_none() {
                let rev = self.track_scene();
                let changed = self.has_unsaved_work();
                if let Err(e) = self.recovery.checkpoint(&self.scene, self.path.as_deref(), changed, rev, std::time::Instant::now()) {
                    self.notice = Some(format!("Could not save auto-recovery data: {e}"));
                }
            }
        }
        self.test_hooks(&ctx);
        let title = format!(
            "MELTCAD — {}{}",
            self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into()),
            if self.dirty { " *" } else { "" }
        );
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

        assert_eq!(app.scene.shape_count(), 2);
        assert!(app.dirty);
        assert!(app.pending_file_action.is_some());
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
    fn saving_before_reopening_the_current_file_keeps_the_saved_edit() {
        let path = std::env::temp_dir().join(format!("meltcad_reopen_guard_{}.meltcad", std::process::id()));
        std::fs::write(&path, Scene::default().to_json()).expect("initial file");
        let ctx = egui::Context::default();
        let mut app = App::new(true, Some(path.clone()));
        app.add_shape(&ctx, Kind::Box);
        app.settle(&ctx);

        app.open_path(path.clone());
        app.resolve_pending_file_action(&ctx, PendingFileDecision::Save);

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
        app.move_layer(Pick::Shape(0, 1), Pick::Group(1), false);
        assert_eq!((app.scene.groups[0].shapes.len(), app.scene.groups[1].shapes.len()), (2, 1));
        app.move_layer(Pick::Group(1), Pick::Group(0), false);
        assert_eq!(app.scene.groups[0].shapes.len(), 1);
    }

    #[test]
    fn a_group_can_be_duplicated_and_the_copy_is_selected() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Group(0));
        app.duplicate();
        assert_eq!(app.scene.groups.len(), 2);
        assert_eq!(app.sel, Some(Pick::Group(1)));
    }
}
