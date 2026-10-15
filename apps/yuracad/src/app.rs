//! The window: top bar, shape palette, viewport, side panels, shortcuts, undo and files.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{Color32, RichText, Ui};

use crate::camera::Camera;
use crate::model::{Group, Kind, MAX_GROUPS, MAX_SHAPES, Material, Op, Pick, Scene, Shape};
use crate::sdf::Field;
use crate::view::{Drag, Tool, centre_of};
use crate::{export, glyphs, style};

const UNDO_LIMIT: usize = 200;

pub struct App {
    pub scene: Scene,
    /// The scene as of the last finished edit (what undo returns to).
    committed: Scene,
    undo: Vec<Scene>,
    redo: Vec<Scene>,
    pub cam: Camera,
    pub sel: Option<Pick>,
    pub tool: Tool,
    /// What a new shape does: add to the group or carve from it.
    pub new_op: Op,
    pub drag: Option<Drag>,
    pub srgb: bool,
    pub last_view_height: f32,
    field: Field,
    field_scene: Scene,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    status: (String, f64),
    pub renaming: Option<Pick>,
    pub folded: HashSet<usize>,
    pub export_quality: usize,
    pub export_smooth: usize,
    shot: Option<ShotPlan>,
}

/// Screenshot and quit, for checking the picture without a person (`YURACAD_SHOT=file.ppm`).
struct ShotPlan {
    path: PathBuf,
    frames: u32,
    asked: bool,
}

impl App {
    pub fn new(srgb: bool) -> App {
        let scene = Scene::default();
        let mut app = App {
            committed: scene.clone(),
            field: Field::new(&scene),
            field_scene: scene.clone(),
            scene,
            undo: Vec::new(),
            redo: Vec::new(),
            cam: Camera::default(),
            sel: Some(Pick::Shape(0, 0)),
            tool: Tool::Move,
            new_op: Op::Add,
            drag: None,
            srgb,
            last_view_height: 600.0,
            path: None,
            dirty: false,
            status: (String::new(), 0.0),
            renaming: None,
            folded: HashSet::new(),
            export_quality: 110,
            export_smooth: 2,
            shot: std::env::var_os("YURACAD_SHOT").map(|p| ShotPlan { path: PathBuf::from(p), frames: 0, asked: false }),
        };
        if let Some(p) = std::env::args().nth(1) {
            app.open_path(PathBuf::from(p));
        }
        app
    }

    pub fn field(&self) -> &Field {
        &self.field
    }

    pub fn say(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.status = (text.into(), ctx.input(|i| i.time));
    }

    // ----- editing -----

    /// Undo steps are made here: a change counts as finished once the mouse is up.
    fn settle(&mut self, ctx: &egui::Context) {
        if self.scene != self.committed && !ctx.input(|i| i.pointer.any_down()) {
            self.undo.push(std::mem::replace(&mut self.committed, self.scene.clone()));
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.dirty = true;
        }
        if self.scene != self.field_scene {
            self.field = Field::new(&self.scene);
            self.field_scene = self.scene.clone();
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
        self.scene = scene.clone();
        self.committed = scene;
        self.undo.clear();
        self.redo.clear();
        self.sel = if self.scene.shape(0, 0).is_some() { Some(Pick::Shape(0, 0)) } else { None };
        self.dirty = false;
        self.frame_view(false);
    }

    /// The group a new shape goes into (made when there is none).
    fn target_group(&mut self) -> Option<usize> {
        let g = match self.sel {
            Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) => Some(g),
            None => self.scene.groups.len().checked_sub(1),
        };
        match g {
            Some(g) if g < self.scene.groups.len() => Some(g),
            _ => {
                self.scene.groups.push(Group::new("グループ 1"));
                Some(self.scene.groups.len() - 1)
            }
        }
    }

    pub fn add_shape(&mut self, ctx: &egui::Context, kind: Kind) {
        if self.scene.shape_count() >= MAX_SHAPES {
            self.say(ctx, format!("形は{MAX_SHAPES}個までです"));
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
        };
        let pos = match self.sel.and_then(|p| centre_of(&self.scene, Some(p))) {
            Some(c) if self.scene.shape_count() > 0 => [c[0] + 18.0, c[1] + 6.0, c[2].max(rest) + 4.0],
            _ => [self.cam.target[0], self.cam.target[1], rest],
        };
        let mut sh = Shape::new(kind, pos, self.next_material());
        sh.op = self.new_op;
        if self.new_op == Op::Subtract {
            sh.material = Material { color: [0.9, 0.35, 0.4], roughness: 0.6, metallic: 0.0 };
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
            self.scene.groups.push(Group::new(format!("グループ {n}")));
            self.sel = Some(Pick::Group(self.scene.groups.len() - 1));
        }
    }

    pub fn duplicate(&mut self) {
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape_count() < MAX_SHAPES => {
                if let Some(mut c) = self.scene.shape(g, s).cloned() {
                    c.pos = [c.pos[0] + 8.0, c.pos[1] + 8.0, c.pos[2]];
                    c.name = format!("{} コピー", c.name);
                    self.scene.groups[g].shapes.insert(s + 1, c);
                    self.sel = Some(Pick::Shape(g, s + 1));
                }
            }
            Some(Pick::Group(g))
                if self.scene.groups.len() < MAX_GROUPS && self.scene.shape_count() + self.scene.groups[g].shapes.len() <= MAX_SHAPES =>
            {
                let mut c = self.scene.groups[g].clone();
                c.name = format!("{} コピー", c.name);
                for s in &mut c.shapes {
                    s.pos = [s.pos[0] + 8.0, s.pos[1] + 8.0, s.pos[2]];
                }
                self.scene.groups.insert(g + 1, c);
                self.sel = Some(Pick::Group(g + 1));
            }
            _ => {}
        }
    }

    pub fn delete_selected(&mut self) {
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
                self.scene.groups.remove(g);
                self.sel = None;
            }
            _ => {}
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
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Scene::from_json(&t)) {
            Ok(s) => {
                self.new_scene(s);
                self.path = Some(path);
            }
            Err(e) => self.status = (format!("開けませんでした: {e}"), 0.0),
        }
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(p) = rfd::FileDialog::new().add_filter("YURACAD", &["yura", "json"]).pick_file() {
            self.open_path(p);
            self.say(ctx, "開きました");
        }
    }

    fn save(&mut self, ctx: &egui::Context, ask: bool) {
        let path = match (&self.path, ask) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("YURACAD", &["yura"]).set_file_name("作品.yura").save_file(),
        };
        let Some(path) = path else { return };
        match std::fs::write(&path, self.scene.to_json()) {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                self.say(ctx, "保存しました");
            }
            Err(e) => self.say(ctx, format!("保存できませんでした: {e}")),
        }
    }

    fn export_stl(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("STL", &["stl"]).set_file_name("作品.stl").save_file() else { return };
        let res = export::mesh(&self.scene, self.export_quality, self.export_smooth)
            .map(|m| export::stl_binary(&m, "YURACAD"))
            .and_then(|b| export::write_stl(&path, &b));
        match res {
            Ok(()) => self.say(ctx, "STLを書き出しました"),
            Err(e) => self.say(ctx, format!("書き出せませんでした: {e}")),
        }
    }

    // ----- shortcuts -----

    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, Modifiers};
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
        let plain = Modifiers::NONE;
        if hit(plain, Key::Delete) || hit(plain, Key::Backspace) {
            self.delete_selected();
        }
        for (k, t) in [(Key::Q, Tool::Select), (Key::W, Tool::Move), (Key::E, Tool::Rotate), (Key::R, Tool::Scale)] {
            if hit(plain, k) {
                self.tool = t;
            }
        }
        if hit(plain, Key::Tab) {
            self.flip_op();
        }
        if hit(plain, Key::F) {
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
                self.cam.look(yaw, pitch);
            }
        }
        for (k, yaw, pitch) in [(Key::Num1, pi, 0.0), (Key::Num3, -pi / 2.0, 0.0), (Key::Num7, self.cam.yaw, -pi / 2.0)] {
            if v(k, true) {
                self.cam.look(yaw, pitch);
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
        egui::Panel::top("top").exact_size(46.0).frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(10, 5))).show(
            ui,
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("YURACAD").size(17.0).strong().color(style::ACCENT));
                    ui.add_space(10.0);
                    if style::icon_button(ui, 34.0, "開く (Ctrl+O)", false, glyphs::open).clicked() {
                        self.open_dialog(&ctx);
                    }
                    if style::icon_button(ui, 34.0, "保存 (Ctrl+S)", false, glyphs::save).clicked() {
                        self.save(&ctx, false);
                    }
                    ui.separator();
                    if style::icon_button(ui, 34.0, "元に戻す (Ctrl+Z)", false, glyphs::undo).clicked() {
                        self.undo();
                    }
                    if style::icon_button(ui, 34.0, "やり直し (Ctrl+Shift+Z)", false, glyphs::redo).clicked() {
                        self.redo();
                    }
                    ui.separator();
                    for (t, label, tip) in [
                        (Tool::Select, "選ぶ", "選ぶ (Q)"),
                        (Tool::Move, "動かす", "動かす (W)"),
                        (Tool::Rotate, "回す", "回す (E)"),
                        (Tool::Scale, "大きさ", "大きさ (R)"),
                    ] {
                        if style::pill(ui, label, self.tool == t).on_hover_text(tip).clicked() {
                            self.tool = t;
                        }
                    }
                    ui.separator();
                    if style::pill(ui, if self.cam.ortho { "平行" } else { "遠近" }, false).on_hover_text("遠近 / 平行 (5)").clicked() {
                        self.cam.ortho = !self.cam.ortho;
                    }
                    if style::pill(ui, "全体", false).on_hover_text("全体を見る (Home)").clicked() {
                        self.frame_view(false);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("STLで書き出す").strong().color(Color32::WHITE))
                                    .fill(style::ACCENT)
                                    .corner_radius(10),
                            )
                            .on_hover_text("3Dプリント用 (Ctrl+E)")
                            .clicked()
                        {
                            self.export_stl(&ctx);
                        }
                        egui::ComboBox::from_id_salt("quality")
                            .selected_text(match self.export_quality {
                                0..=79 => "ふつう",
                                80..=139 => "きれい",
                                _ => "とてもきれい",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.export_quality, 64, "ふつう (軽い)");
                                ui.selectable_value(&mut self.export_quality, 110, "きれい");
                                ui.selectable_value(&mut self.export_quality, 170, "とてもきれい (重い)");
                            });
                        ui.label(RichText::new("書き出し").color(style::DIM).size(12.0));
                    });
                });
            },
        );
    }

    fn palette(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::left("palette")
            .exact_size(78.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(8, 10)))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("形を足す").size(11.5).color(style::DIM));
                    for k in Kind::ALL {
                        let r = style::icon_button(ui, 54.0, k.label(), false, |p, r, ink| glyphs::kind(p, r, k, ink, Color32::from_white_alpha(18)));
                        if r.clicked() {
                            self.add_shape(&ctx, k);
                        }
                        ui.label(RichText::new(k.label()).size(10.5).color(style::DIM));
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(RichText::new("つぎの形は").size(11.0).color(style::DIM));
                    for op in [Op::Add, Op::Subtract] {
                        let on = self.new_op == op;
                        let col = if on {
                            Color32::WHITE
                        } else if op == Op::Add {
                            style::ADD
                        } else {
                            style::CUT
                        };
                        if style::icon_button(
                            ui,
                            40.0,
                            if op == Op::Add { "足す (くっつく)" } else { "削る (くり抜く)" },
                            on,
                            |p, r, _| glyphs::op(p, r, op, col),
                        )
                        .clicked()
                        {
                            self.new_op = op;
                        }
                    }
                    ui.add_space(6.0);
                    if style::icon_button(ui, 40.0, "グループを作る", false, glyphs::group).clicked() {
                        self.add_group();
                    }
                });
            });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let now = ui.input(|i| i.time);
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(style::PANEL_DARK).inner_margin(egui::Margin::symmetric(12, 3)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let note = if now - self.status.1 < 5.0 { self.status.0.clone() } else { String::new() };
                    let hint = match self.tool {
                        Tool::Select => "クリックで選ぶ",
                        Tool::Move => "矢印で動かす (Ctrlで1mmずつ)",
                        Tool::Rotate => "輪っかで回す (Ctrlで15度ずつ)",
                        Tool::Scale => "四角で大きさを変える",
                    };
                    ui.label(RichText::new(if note.is_empty() { hint.to_string() } else { note }).size(11.5).color(style::DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "名前なし".into());
                        ui.label(
                            RichText::new(format!("{}{}　形 {}", name, if self.dirty { " *" } else { "" }, self.scene.shape_count()))
                                .size(11.5)
                                .color(style::DIM),
                        );
                    });
                });
            });
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(plan) = &mut self.shot else { return };
        plan.frames += 1;
        if plan.frames > 8 && !plan.asked {
            plan.asked = true;
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
            let _ = std::fs::write(&plan.path, out);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.fix_selection();
        self.keys(&ctx);
        self.top_bar(ui);
        self.status_bar(ui);
        self.palette(ui);
        self.side(ui);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(52, 52, 56))).show(ui, |ui| {
            self.last_view_height = ui.available_height();
            self.viewport(ui);
        });
        self.settle(&ctx);
        self.screenshot(&ctx);
        let title = format!(
            "YURACAD — {}{}",
            self.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "名前なし".into()),
            if self.dirty { " *" } else { "" }
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }
}
