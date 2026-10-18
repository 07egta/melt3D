//! The "スケッチ" tool: draw a flat shape on a plane in the viewport, then turn it into a solid
//! (押し出し / 回転体 / ロフト / スイープ). The geometry is in `sketch.rs`; this file is the glue to the
//! mouse, the keys, the floating card, the painting and the icon.
//!
//! The mouse and the keys work like this (also on the card):
//!  - Points snap to a 1 mm grid. Shift = 10 mm, Ctrl (Command on a Mac) = 0.1 mm.
//!  - ポリゴン / スプライン / 線: click points. Click the first point, press Enter or double-click to
//!    finish. Backspace takes the last point back, Esc starts over (and, with nothing drawn, ends the tool).
//!  - 四角 / 円: drag (or click twice) from one corner / from the middle.
//!  - Alt + drag, the middle button and the right button still turn and slide the view.
//!
//! Editing a made shape (button "スケッチを編集" in the shape panel) shows its outline on its plane:
//! drag a point, click on a line to add a point, click a point and press Delete to remove it.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape as Gfx, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::curve_tool::Input;
use crate::model::{Kind, MAX_OUTLINE, MAX_PATH, MAX_SHAPES, Material, Op, Pick, Shape};
use crate::sdf::V3;
use crate::sketch::{self, Frame, SketchError, Which};
use crate::style;

/// Pixels within which a click on the first point closes the outline.
const CLOSE_PX: f32 = 11.0;
/// Pixels within which the mouse is on a point of an outline being edited.
const POINT_PX: f32 = 10.0;
/// Pixels within which the mouse is on a line of an outline being edited.
const EDGE_PX: f32 = 7.0;
/// Where the viewport's view cube is (clicks there are not ours).
const GNOMON_PX: f32 = 58.0;

// ------------------------------------------------------------------ what the tool can be set to

/// The plane the sketch is drawn on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaneSel {
    Floor,
    Front,
    Side,
    /// A level plane through the top of the selected shape.
    OnShape,
}

impl PlaneSel {
    pub const ALL: [PlaneSel; 4] = [PlaneSel::Floor, PlaneSel::Front, PlaneSel::Side, PlaneSel::OnShape];
    pub fn label(self) -> &'static str {
        match self {
            PlaneSel::Floor => "床",
            PlaneSel::Front => "正面",
            PlaneSel::Side => "横",
            PlaneSel::OnShape => "えらんだ形の上",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            PlaneSel::Floor => "床 (XY) の上にかく。真上から見ます",
            PlaneSel::Front => "正面 (XZ) にかく。手前に向かって押し出します",
            PlaneSel::Side => "横 (YZ) にかく。右がわに向かって押し出します",
            PlaneSel::OnShape => "えらんでいる形のてっぺんの高さで、水平な面にかく",
        }
    }
}

/// What the mouse draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pen {
    Polygon,
    Rect,
    Circle,
    Spline,
    /// An open line: a path for a sweep.
    Line,
}

impl Pen {
    pub const ALL: [Pen; 5] = [Pen::Polygon, Pen::Rect, Pen::Circle, Pen::Spline, Pen::Line];
    pub fn label(self) -> &'static str {
        match self {
            Pen::Polygon => "ポリゴン",
            Pen::Rect => "四角",
            Pen::Circle => "円",
            Pen::Spline => "スプライン",
            Pen::Line => "線",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            Pen::Polygon => "クリックで角をうって、多角形をかく",
            Pen::Rect => "ドラッグ (または2回クリック) で、四角をかく",
            Pen::Circle => "まん中から外へドラッグして、円をかく",
            Pen::Spline => "クリックした点を通る、なめらかな輪かくをかく",
            Pen::Line => "スイープの通り道になる、ひらいた線をかく",
        }
    }
}

/// What the finished profile becomes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Make {
    Extrude,
    Revolve,
    Loft,
    Sweep,
}

impl Make {
    pub const ALL: [Make; 4] = [Make::Extrude, Make::Revolve, Make::Loft, Make::Sweep];
    pub fn label(self) -> &'static str {
        match self {
            Make::Extrude => "押し出し",
            Make::Revolve => "回転体",
            Make::Loft => "ロフト",
            Make::Sweep => "スイープ",
        }
    }
    fn kind(self) -> Kind {
        match self {
            Make::Extrude => Kind::Extrude,
            Make::Revolve => Kind::Revolve,
            Make::Loft => Kind::Loft,
            Make::Sweep => Kind::Sweep,
        }
    }
}

/// Where in the workflow the sketch is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Drawing a profile (or a line to keep for a sweep).
    Draw,
    /// A closed profile is ready: the card asks what to make of it.
    Ready,
    /// Drawing the loft's top profile.
    LoftTop,
    /// Drawing the sweep's path.
    SweepPath,
}

/// How the loft's top profile is found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoftTop {
    /// A copy of the bottom one, scaled.
    Scaled,
    /// A second profile drawn on the same plane.
    Drawn,
}

/// What a drawing gesture produced.
#[derive(Clone, PartialEq, Debug)]
pub enum Finished {
    /// A closed, checked, counter-clockwise outline.
    Profile(Vec<[f32; 2]>),
    /// An open line (corners or a smooth curve), in plane coordinates.
    Line(Vec<[f32; 2]>),
}

// ------------------------------------------------------------------ the state

/// The outline of a made shape being edited in the viewport.
#[derive(Clone, Debug)]
pub struct Edit {
    pub g: usize,
    pub s: usize,
    pub which: Which,
    /// The chosen point (Delete removes it).
    pub sel: Option<usize>,
    pub drag: Option<DragPoint>,
    pub hover: Option<Hover>,
}

#[derive(Clone, Debug)]
pub struct DragPoint {
    pub idx: usize,
    /// The outline before the drag (put back when the result would cross itself).
    pub before: Vec<[f32; 2]>,
}

/// What the mouse is on while editing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hover {
    Point(usize),
    /// A line after corner `.0`; a click adds a point at `.1` (outline coordinates).
    Edge(usize, [f32; 2]),
}

pub struct State {
    /// The sketch tool is on.
    pub on: bool,
    pub plane: PlaneSel,
    /// The plane being drawn on.
    pub frame: Frame,
    pub pen: Pen,
    /// Splines and lines are curves (true) or have corners (false).
    pub smooth: bool,
    pub stage: Stage,
    /// The points clicked so far.
    pub pts: Vec<[f32; 2]>,
    /// The first corner of a rectangle / the middle of a circle.
    pub anchor: Option<[f32; 2]>,
    /// A rectangle or circle is being dragged out.
    pub dragging: bool,
    /// Where the mouse is on the plane (snapped).
    pub cursor: Option<[f32; 2]>,
    /// Where the mouse is on the screen (None when it is elsewhere).
    pub mouse: Option<Pos2>,
    /// The finished profile and the plane it was drawn on.
    pub profile: Vec<[f32; 2]>,
    pub pframe: Frame,
    /// The loft's drawn top profile.
    pub profile2: Vec<[f32; 2]>,
    /// The last line drawn, in the world (kept for sweeps).
    pub path: Vec<V3>,
    pub make: Make,
    pub height: f32,
    /// Grow against the plane's normal.
    pub flip: bool,
    pub loft_top: LoftTop,
    pub loft_scale: f32,
    pub sweep_scale: f32,
    /// A friendly message under the tool name.
    pub msg: String,
    pub edit: Option<Edit>,
}

impl Default for State {
    fn default() -> Self {
        State {
            on: false,
            plane: PlaneSel::Floor,
            frame: Frame::floor(),
            pen: Pen::Polygon,
            smooth: true,
            stage: Stage::Draw,
            pts: Vec::new(),
            anchor: None,
            dragging: false,
            cursor: None,
            mouse: None,
            profile: Vec::new(),
            pframe: Frame::floor(),
            profile2: Vec::new(),
            path: Vec::new(),
            make: Make::Extrude,
            height: 20.0,
            flip: false,
            loft_top: LoftTop::Scaled,
            loft_scale: 0.5,
            sweep_scale: 1.0,
            msg: String::new(),
            edit: None,
        }
    }
}

/// The grid step for the modifier keys held: 1 mm, Shift 10 mm, Ctrl (Command) 0.1 mm.
pub fn step_for(mods: egui::Modifiers) -> f32 {
    if mods.command {
        0.1
    } else if mods.shift {
        10.0
    } else {
        1.0
    }
}

fn same(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

impl State {
    /// The pen in use (a sweep path is always a line).
    pub fn active_pen(&self) -> Pen {
        if self.stage == Stage::SweepPath { Pen::Line } else { self.pen }
    }

    /// Forget the gesture in progress.
    pub fn clear_drawing(&mut self) {
        self.pts.clear();
        self.anchor = None;
        self.dragging = false;
        self.cursor = None;
        self.mouse = None;
    }

    /// Back to a blank page (the settings stay).
    pub fn reset(&mut self) {
        self.clear_drawing();
        self.stage = Stage::Draw;
        self.profile.clear();
        self.profile2.clear();
        self.loft_top = LoftTop::Scaled;
        self.msg.clear();
    }

    /// Corners (not a smooth curve) with the live crossing check?
    fn corners(&self) -> bool {
        match self.active_pen() {
            Pen::Polygon => true,
            Pen::Spline => !self.smooth,
            _ => false,
        }
    }

    fn add_point(&mut self, p: [f32; 2]) {
        let limit = if self.active_pen() == Pen::Line { MAX_PATH } else { MAX_OUTLINE };
        if self.pts.last().is_some_and(|l| same(*l, p)) {
            return;
        }
        if self.pts.len() >= limit {
            self.msg = format!("点は{limit}こまでです");
            return;
        }
        if self.corners()
            && let Some(last) = self.pts.last().copied()
            && self.pts.len() >= 2
            && sketch::crosses_polyline(&self.pts, last, p, false)
        {
            self.msg = "そこへ線をひくと交差します。ほかの場所をクリックしてください".to_string();
            return;
        }
        self.msg.clear();
        self.pts.push(p);
    }

    /// Would a click at `p` (cursor) draw a line that crosses? For the red preview.
    pub fn would_cross(&self, p: [f32; 2]) -> bool {
        self.corners() && self.pts.len() >= 2 && self.pts.last().is_some_and(|l| sketch::crosses_polyline(&self.pts, *l, p, false))
    }

    /// Finish the line / outline of the points clicked.
    fn finish_points(&mut self) -> Option<Finished> {
        match self.active_pen() {
            Pen::Polygon | Pen::Spline => match sketch::closed_profile(&self.pts, self.active_pen() == Pen::Spline && self.smooth) {
                Ok(p) => {
                    self.clear_drawing();
                    self.msg.clear();
                    Some(Finished::Profile(p))
                }
                Err(e) => {
                    self.msg = e.message();
                    None
                }
            },
            Pen::Line => match sketch::open_line(&self.pts, self.smooth) {
                Ok(l) => {
                    self.clear_drawing();
                    self.msg.clear();
                    Some(Finished::Line(l))
                }
                Err(e) => {
                    self.msg = e.message();
                    None
                }
            },
            Pen::Rect | Pen::Circle => None,
        }
    }

    fn finish_shape(&mut self, a: [f32; 2], b: [f32; 2]) -> Option<Finished> {
        let res = if self.active_pen() == Pen::Rect { sketch::rect_profile(a, b) } else { sketch::circle_profile(a, b) };
        self.anchor = None;
        self.dragging = false;
        match res {
            Ok(p) => {
                self.msg.clear();
                Some(Finished::Profile(p))
            }
            Err(e) => {
                self.msg = e.message();
                None
            }
        }
    }

    /// A click at `p` on the plane. `close` = the click is on the first point.
    pub fn click(&mut self, p: [f32; 2], close: bool) -> Option<Finished> {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        match self.active_pen() {
            Pen::Polygon | Pen::Spline => {
                if self.pts.len() >= 3 && (close || self.pts.first().is_some_and(|f| same(*f, p))) {
                    return self.finish_points();
                }
                self.add_point(p);
                None
            }
            Pen::Line => {
                self.add_point(p);
                None
            }
            Pen::Rect | Pen::Circle => match self.anchor.take() {
                None => {
                    self.anchor = Some(p);
                    self.msg.clear();
                    None
                }
                Some(a) => self.finish_shape(a, p),
            },
        }
    }

    /// The button went down and the mouse is dragging (rectangles and circles).
    pub fn press(&mut self, p: [f32; 2]) {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return;
        }
        if matches!(self.active_pen(), Pen::Rect | Pen::Circle) {
            if self.anchor.is_none() {
                self.anchor = Some(p);
            }
            self.dragging = true;
            self.msg.clear();
        }
    }

    /// Abandon a rectangle/circle drag lost outside the viewport.
    /// Do not leave a stale two-click anchor behind.
    pub fn cancel_drag(&mut self) {
        self.dragging = false;
        self.anchor = None;
        self.cursor = None;
        self.mouse = None;
    }

    /// The button came up at `p` after a drag.
    pub fn release(&mut self, p: [f32; 2]) -> Option<Finished> {
        if !self.dragging {
            return None;
        }
        self.dragging = false;
        let a = self.anchor?;
        // a drag that went nowhere is the first click of a two-click shape
        if (a[0] - p[0]).hypot(a[1] - p[1]) < sketch::MIN_SIDE {
            return None;
        }
        self.finish_shape(a, p)
    }

    /// Enter (or a double click): close the outline / end the line.
    pub fn enter(&mut self) -> Option<Finished> {
        match self.active_pen() {
            Pen::Rect | Pen::Circle => match (self.anchor, self.cursor) {
                (Some(a), Some(b)) if !same(a, b) => self.finish_shape(a, b),
                _ => None,
            },
            _ if self.pts.is_empty() => None,
            _ => self.finish_points(),
        }
    }

    /// Backspace: take the last point (or the first corner) back.
    pub fn back(&mut self) {
        if self.anchor.is_some() {
            self.anchor = None;
            self.dragging = false;
        } else {
            self.pts.pop();
        }
        self.msg.clear();
    }

    /// Esc. True when there was nothing to undo here (the tool itself should step back or end).
    pub fn escape(&mut self) -> bool {
        if !self.pts.is_empty() || self.anchor.is_some() {
            self.clear_drawing();
            self.msg.clear();
            return false;
        }
        match self.stage {
            Stage::Ready => {
                self.stage = Stage::Draw;
                self.profile.clear();
                self.profile2.clear();
                self.msg.clear();
                false
            }
            Stage::LoftTop | Stage::SweepPath => {
                self.stage = Stage::Ready;
                self.msg.clear();
                false
            }
            Stage::Draw => true,
        }
    }
}

// ------------------------------------------------------------------ the App side: turning it on, the plane, the camera

const COLORS: [[f32; 3]; 6] =
    [[0.93, 0.56, 0.45], [0.60, 0.72, 0.98], [0.62, 0.90, 0.74], [0.96, 0.80, 0.45], [0.76, 0.66, 0.96], [0.96, 0.66, 0.82]];

impl App {
    /// Is the sketch tool on, or an outline being edited? (The gizmo and the node editing stay out of the way.)
    pub fn sketch_active(&self) -> bool {
        self.sk.on || self.sk.edit.is_some()
    }

    /// Switch the sketch tool on or off. It excludes the other drawing tools.
    pub fn set_sketch_tool(&mut self, on: bool) {
        self.sk.edit = None;
        self.sk.reset();
        self.sk.on = on;
        if on {
            self.finish_drawing();
            self.ct.drawing = false;
            self.set_pipe_tool(false);
            self.sk.frame = self.sketch_plane();
            let f = self.sk.frame;
            self.sketch_fly(&f);
        }
    }

    /// Called by the other drawing tools when they start.
    pub fn sketch_off(&mut self) {
        if self.sk.on || self.sk.edit.is_some() {
            self.sk.edit = None;
            self.sk.reset();
            self.sk.on = false;
        }
    }

    /// The button in the tool strip.
    pub fn sketch_button(&mut self, ui: &mut Ui) {
        let on = self.sk.on;
        if style::icon_button(ui, 34.0, "スケッチ (平面に絵をかいて、押し出し・回転体・ロフト・スイープにする)", on, glyph).clicked()
        {
            self.set_sketch_tool(!on);
        }
    }

    /// The top of the selected shape (or of the highest shape in the selected group).
    fn selection_top(&self) -> Option<f32> {
        let tops: Vec<f32> = crate::view::targets(&self.scene, self.sel)
            .into_iter()
            .filter_map(|(g, s)| self.scene.shape(g, s))
            .filter(|s| s.visible)
            .map(sketch::top_of)
            .collect();
        tops.into_iter().reduce(f32::max)
    }

    /// The frame of the chosen plane right now.
    pub fn sketch_plane(&self) -> Frame {
        match self.sk.plane {
            PlaneSel::Floor => Frame::floor(),
            PlaneSel::Front => Frame::front(),
            PlaneSel::Side => Frame::side(),
            PlaneSel::OnShape => Frame::horizontal(self.selection_top().unwrap_or(0.0)),
        }
    }

    /// Glide the camera to look straight at a plane, keeping the plane through the view's middle.
    pub fn sketch_fly(&mut self, f: &Frame) {
        let n = f.normal();
        let (yaw, pitch) = if n[2].abs() > 0.9 {
            (self.cam.yaw, if n[2] > 0.0 { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 })
        } else {
            (n[0].atan2(-n[1]), 0.0)
        };
        let mut goal = self.fly.unwrap_or(self.cam);
        goal.look(yaw, pitch);
        // the target moves onto the plane
        let t = goal.target;
        let off = crate::sdf::dot(crate::sdf::sub(t, f.origin), n);
        goal.target = crate::sdf::sub(t, crate::sdf::mul(n, off));
        self.fly = Some(goal);
    }

    /// Choose a plane (and glide to it).
    pub fn sketch_set_plane(&mut self, plane: PlaneSel) {
        self.sk.plane = plane;
        self.sk.clear_drawing();
        self.sk.frame = self.sketch_plane();
        let f = self.sk.frame;
        self.sketch_fly(&f);
    }

    /// Where the screen position `p` is on the plane `f` (snapped to `step`).
    pub fn sketch_cursor(&self, rect: Rect, p: Pos2, step: f32, f: &Frame) -> Option<[f32; 2]> {
        let (o, d) = self.cam.ray_at(rect, p);
        f.hit(o, d).map(|q| sketch::snap(q, step))
    }

    /// Where a point of plane `f` is on the screen.
    fn sketch_px(&self, rect: Rect, f: &Frame, q: [f32; 2]) -> Option<Pos2> {
        self.cam.project(rect, f.to_world(q))
    }

    // ----- what a finished gesture does -----

    /// A profile or a line has been drawn.
    pub fn sketch_done(&mut self, done: Finished) {
        match done {
            Finished::Profile(p) => {
                if self.sk.stage == Stage::LoftTop {
                    self.sk.profile2 = p;
                    self.sk.loft_top = LoftTop::Drawn;
                    self.sk.make = Make::Loft;
                } else {
                    self.sk.profile = p;
                    self.sk.pframe = self.sk.frame;
                    self.sk.profile2.clear();
                    self.sk.loft_top = LoftTop::Scaled;
                    if self.sk.make == Make::Revolve && !self.sk.pframe.is_vertical() {
                        self.sk.make = Make::Extrude;
                    }
                }
                self.sk.stage = Stage::Ready;
            }
            Finished::Line(l) => {
                let f = self.sk.frame;
                self.sk.path = l.iter().map(|q| f.to_world(*q)).collect();
                if self.sk.stage == Stage::SweepPath {
                    self.sk.stage = Stage::Ready;
                    self.sk.make = Make::Sweep;
                    self.sk.frame = self.sk.pframe;
                } else {
                    self.sk.msg = format!("線を{}点でおぼえました。輪かくをかいて「スイープ」で使えます", self.sk.path.len());
                }
            }
        }
    }

    /// Make the shape the card is set to. True when it was made.
    pub fn sketch_make(&mut self, ctx: &egui::Context) -> bool {
        if self.scene.shape_count() >= MAX_SHAPES {
            self.sk.msg = format!("形は{MAX_SHAPES}個までです");
            return false;
        }
        let st = &self.sk;
        let built = match st.make {
            Make::Extrude => sketch::extrude(&st.pframe, &st.profile, st.height, st.flip),
            Make::Revolve => sketch::revolve(&st.pframe, &st.profile),
            Make::Loft => {
                let top = match st.loft_top {
                    LoftTop::Scaled => sketch::scaled(&st.profile, st.loft_scale),
                    LoftTop::Drawn => st.profile2.clone(),
                };
                sketch::loft(&st.pframe, &st.profile, &top, st.height, st.flip)
            }
            Make::Sweep => sketch::sweep(&st.profile, &st.path, st.sweep_scale),
        };
        let built = match built {
            Ok(b) => b,
            Err(e) => {
                self.sk.msg = e.message();
                return false;
            }
        };
        let make = self.sk.make;
        let Some(g) = self.target_group() else { return false };
        let mut color = COLORS[self.scene.shape_count() % COLORS.len()];
        if self.new_op == Op::Subtract {
            color = [0.9, 0.35, 0.4];
        }
        let mut sh = Shape::new(make.kind(), built.pos, Material { color, ..Material::default() });
        sketch::apply(&built, &mut sh);
        sh.name = make.label().to_string();
        sh.op = self.new_op;
        sh.blend = 2.0;
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        group.shapes.push(sh);
        let idx = group.shapes.len() - 1;
        self.sel = Some(Pick::Shape(g, idx));
        self.sk.reset();
        self.sk.frame = self.sketch_plane();
        self.say(ctx, format!("{}を作りました（「スケッチを編集」で輪かくをなおせます）", make.label()));
        true
    }

    // ----- the mouse -----

    /// The mouse while the sketch tool is on or an outline is being edited. None = not active.
    pub fn sketch_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Option<Input> {
        if !self.sketch_active() {
            return None;
        }
        Some(if self.sk.edit.is_some() {
            self.sketch_edit_input(ui, resp, rect, pos, mods)
        } else {
            self.sketch_draw_input(ui, resp, rect, pos, mods)
        })
    }

    fn sketch_draw_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Input {
        let mut out = Input::default();
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < GNOMON_PX;
        match self.sk.stage {
            Stage::Ready | Stage::LoftTop => self.sk.frame = self.sk.pframe,
            _ if self.sk.pts.is_empty() && self.sk.anchor.is_none() => self.sk.frame = self.sketch_plane(),
            _ => {}
        }
        let frame = self.sk.frame;
        let step = step_for(mods);
        let mouse = pos.filter(|p| resp.hovered() && !over_gnomon(*p));
        // Losing the pointer during a shape gesture cancels it, rather than
        // reusing its first corner on the next click.
        // egui retains interact_pos for the PointerGone frame. A released touch may
        // still finish at that position; only a gesture that is still held is lost.
        if self.sk.dragging && ui.input(|i| i.pointer.primary_down() && !i.pointer.has_pointer()) {
            self.sk.cancel_drag();
        }
        self.sk.mouse = mouse;
        self.sk.cursor = mouse.and_then(|p| self.sketch_cursor(rect, p, step, &frame));
        out.over = self.sk.cursor.is_some();
        if self.sk.stage == Stage::Ready {
            // nothing to draw: clicks must not pick things behind the card
            out.taken = mouse.is_some() && resp.clicked_by(egui::PointerButton::Primary);
            return out;
        }
        if mods.alt {
            return out;
        }
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let mut done = None;
        if matches!(self.sk.active_pen(), Pen::Rect | Pen::Circle) {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(o) = ui.input(|i| i.pointer.press_origin()).or(pos).filter(|o| !over_gnomon(*o))
                && let Some(q) = self.sketch_cursor(rect, o, step, &frame)
            {
                self.sk.press(q);
            }
            if self.sk.dragging {
                out.taken = true;
                out.busy = true;
                if !primary_down || resp.drag_stopped() {
                    match self.sk.cursor {
                        Some(q) => done = self.sk.release(q),
                        None => self.sk.cancel_drag(),
                    }
                }
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary)
            && let (Some(m), Some(q)) = (mouse, self.sk.cursor)
        {
            let first = self.sk.pts.first().and_then(|f| self.sketch_px(rect, &frame, *f));
            let close = self.sk.pts.len() >= 3 && first.is_some_and(|f| (f - m).length() <= CLOSE_PX);
            done = done.or_else(|| self.sk.click(q, close));
            out.taken = true;
        }
        // A quick first point followed by a double-click at the endpoint can
        // be reported as a triple-click by egui. Both finish the same gesture.
        if (resp.double_clicked_by(egui::PointerButton::Primary) || resp.triple_clicked_by(egui::PointerButton::Primary)) && mouse.is_some() {
            done = done.or_else(|| self.sk.enter());
            out.taken = true;
        }
        if let Some(d) = done {
            self.sketch_done(d);
        }
        // Every primary sketch gesture belongs to the sketch, including drags
        // which have not produced a point. Never fall through to object picking.
        if !mods.alt
            && (resp.drag_started_by(egui::PointerButton::Primary)
                || resp.dragged_by(egui::PointerButton::Primary)
                || resp.drag_stopped_by(egui::PointerButton::Primary)
                || resp.clicked_by(egui::PointerButton::Primary)
                || resp.double_clicked_by(egui::PointerButton::Primary))
        {
            out.taken = true;
        }
        out
    }

    // ----- the keys -----

    /// Enter, Backspace / Delete and Esc while the tool is on. Call before the other shortcuts.
    /// Delete passes through to shape deletion only when drawing has no pending point.
    pub fn sketch_keys(&mut self, ctx: &egui::Context) {
        if !self.sketch_active() {
            return;
        }
        use egui::{Key, Modifiers};
        let hit = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let sketch_uses_delete = self.sk.edit.is_some() || !self.sk.pts.is_empty() || self.sk.anchor.is_some();
        let del = (sketch_uses_delete && hit(Key::Delete)) | hit(Key::Backspace);
        let enter = hit(Key::Enter);
        let esc = hit(Key::Escape);
        if self.sk.edit.is_some() {
            if esc && self.sk.edit.as_ref().is_some_and(|e| e.drag.is_some()) {
                self.edit_cancel_drag();
                return;
            }
            if del {
                self.sketch_edit_delete();
            }
            if enter || esc {
                self.sk.edit = None;
            }
            return;
        }
        if enter && let Some(d) = self.sk.enter() {
            self.sketch_done(d);
        }
        if del {
            self.sk.back();
        }
        if esc && self.sk.escape() {
            self.set_sketch_tool(false);
        }
    }

    // ----- editing a made shape -----

    /// Start editing the outline of shape (g, s) in the viewport.
    pub fn sketch_edit_begin(&mut self, g: usize, s: usize) {
        let ok = self.scene.shape(g, s).is_some_and(sketch::editable);
        if !ok {
            return;
        }
        self.finish_drawing();
        self.ct.drawing = false;
        self.set_pipe_tool(false);
        self.sk.reset();
        self.sk.on = false;
        self.sel = Some(Pick::Shape(g, s));
        self.sk.edit = Some(Edit { g, s, which: Which::Main, sel: None, drag: None, hover: None });
        if let Some(f) = self.scene.shape(g, s).and_then(|sh| sketch::edit_frame(sh, Which::Main)) {
            self.sketch_fly(&f);
        }
    }

    /// The button in the shape panel: edit this shape's outline (or stop).
    pub fn sketch_edit_button(&mut self, ui: &mut Ui, g: usize, s: usize) {
        let editing = self.sk.edit.as_ref().is_some_and(|e| e.g == g && e.s == s);
        let tip = "この形の輪かくを、画面の中で点をうごかして なおします";
        if style::pill(ui, if editing { "スケッチの編集をおわる" } else { "スケッチを編集" }, editing).on_hover_text(tip).clicked()
        {
            if editing {
                self.sk.edit = None;
            } else {
                self.sketch_edit_begin(g, s);
            }
        }
    }

    /// The frame and outline being edited, or None when the shape is gone (the editing then ends).
    fn edit_target(&mut self) -> Option<(Frame, Vec<[f32; 2]>)> {
        let e = self.sk.edit.as_ref()?;
        let found = self.scene.shape(e.g, e.s).filter(|sh| sketch::editable(sh)).and_then(|sh| {
            let f = sketch::edit_frame(sh, e.which)?;
            Some((f, sketch::outline_of(sh, e.which).clone()))
        });
        if found.is_none() {
            self.sk.edit = None;
        }
        found
    }

    /// What is under the mouse at `m`: a point first, then a line.
    pub fn edit_hit(&self, rect: Rect, m: Pos2) -> Option<Hover> {
        let e = self.sk.edit.as_ref()?;
        let sh = self.scene.shape(e.g, e.s)?;
        let f = sketch::edit_frame(sh, e.which)?;
        let ol = sketch::outline_of(sh, e.which);
        let px: Vec<Option<Pos2>> = ol.iter().map(|q| self.sketch_px(rect, &f, *q)).collect();
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in px.iter().enumerate() {
            if let Some(c) = c {
                let d = (*c - m).length();
                if d <= POINT_PX && best.is_none_or(|b| d < b.1) {
                    best = Some((i, d));
                }
            }
        }
        if let Some((i, _)) = best {
            return Some(Hover::Point(i));
        }
        let n = ol.len();
        let mut edge: Option<(usize, [f32; 2], f32)> = None;
        for i in 0..n {
            let (Some(a), Some(b)) = (px.get(i).copied().flatten(), px.get((i + 1) % n).copied().flatten()) else { continue };
            let ab = b - a;
            let t = ((m - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
            let d = (m - (a + ab * t)).length();
            if d <= EDGE_PX && edge.is_none_or(|b| d < b.2) {
                let (oa, ob) = (ol[i], ol[(i + 1) % n]);
                let at = [oa[0] + (ob[0] - oa[0]) * t, oa[1] + (ob[1] - oa[1]) * t];
                edge = Some((i, [(at[0] * 100.0).round() / 100.0, (at[1] * 100.0).round() / 100.0], d));
            }
        }
        edge.map(|(i, at, _)| Hover::Edge(i, at))
    }

    fn edit_outline_mut(&mut self) -> Option<(&mut Vec<[f32; 2]>, bool)> {
        let e = self.sk.edit.as_ref()?;
        let (g, s, which) = (e.g, e.s, e.which);
        let sh = self.scene.shape_mut(g, s)?;
        let rev = sh.kind == Kind::Revolve;
        Some((sketch::outline_of_mut(sh, which), rev))
    }

    /// The button went down on point `idx`: start moving it.
    pub fn edit_begin_drag(&mut self, idx: usize) -> bool {
        let Some((ol, _)) = self.edit_outline_mut() else { return false };
        if idx >= ol.len() {
            return false;
        }
        let before = ol.clone();
        if let Some(e) = self.sk.edit.as_mut() {
            e.drag = Some(DragPoint { idx, before });
            e.sel = Some(idx);
        }
        true
    }

    /// The dragged point follows the mouse.
    pub fn edit_drag(&mut self, rect: Rect, m: Pos2, step: f32) {
        let Some((f, _)) = self.edit_target() else { return };
        let Some(idx) = self.sk.edit.as_ref().and_then(|e| e.drag.as_ref()).map(|d| d.idx) else { return };
        let Some(q) = self.sketch_cursor(rect, m, step, &f) else { return };
        if let Some((ol, rev)) = self.edit_outline_mut()
            && let Some(p) = ol.get_mut(idx)
        {
            *p = if rev { [q[0].max(0.0), q[1]] } else { q };
        }
    }

    /// Escape during a node drag restores the exact original outline.
    pub fn edit_cancel_drag(&mut self) {
        let Some(d) = self.sk.edit.as_mut().and_then(|e| e.drag.take()) else { return };
        if let Some((ol, _)) = self.edit_outline_mut() {
            *ol = d.before;
        }
        self.sk.msg.clear();
    }

    /// The button came up: keep the new outline unless it now crosses itself.
    pub fn edit_release(&mut self) {
        let Some(d) = self.sk.edit.as_mut().and_then(|e| e.drag.take()) else { return };
        let Some((ol, _)) = self.edit_outline_mut() else { return };
        if sketch::is_simple(&d.before) && !sketch::is_simple(ol) {
            *ol = d.before;
            self.sk.msg = SketchError::SelfIntersect.message();
        } else {
            self.sk.msg.clear();
        }
    }

    /// A click without a drag: choose a point, add one on a line, or choose nothing.
    pub fn edit_click(&mut self, rect: Rect, m: Pos2) {
        match self.edit_hit(rect, m) {
            Some(Hover::Point(i)) => {
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = Some(i);
                }
            }
            Some(Hover::Edge(i, at)) => {
                let added = self.edit_outline_mut().is_some_and(|(ol, rev)| {
                    let at = if rev { [at[0].max(0.0), at[1]] } else { at };
                    sketch::insert_point(ol, i, at)
                });
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = added.then_some(i + 1);
                }
                if !added {
                    self.sk.msg = format!("点は{MAX_OUTLINE}こまでです");
                }
            }
            None => {
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = None;
                }
            }
        }
    }

    /// Delete: remove the chosen point (three stay at least).
    pub fn sketch_edit_delete(&mut self) {
        let Some(i) = self.sk.edit.as_ref().and_then(|e| e.sel) else { return };
        let res = self.edit_outline_mut().map(|(ol, _)| sketch::remove_point(ol, i));
        match res {
            Some(Ok(())) => {
                self.sk.msg.clear();
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = None;
                }
            }
            Some(Err(err)) => self.sk.msg = err.message(),
            None => {}
        }
    }

    fn sketch_edit_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Input {
        let mut out = Input::default();
        if self.edit_target().is_none() {
            return out;
        }
        let step = step_for(mods);
        let primary_down = ui.input(|i| i.pointer.primary_down());
        if self.sk.edit.as_ref().is_some_and(|e| e.drag.is_some()) {
            out.over = true;
            out.taken = true;
            out.busy = true;
            match (primary_down, ui.input(|i| i.pointer.latest_pos())) {
                (true, Some(p)) if rect.contains(p) => self.edit_drag(rect, p, step),
                (true, _) => self.edit_cancel_drag(),
                _ => self.edit_release(),
            }
            return out;
        }
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < GNOMON_PX;
        let mouse = pos.filter(|p| resp.hovered() && !over_gnomon(*p) && !mods.alt);
        let hover = mouse.and_then(|m| self.edit_hit(rect, m));
        if let Some(e) = self.sk.edit.as_mut() {
            e.hover = hover;
        }
        out.over = hover.is_some();
        if mods.alt {
            return out;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pos).filter(|o| !over_gnomon(*o));
            if let Some(Hover::Point(i)) = origin.and_then(|o| self.edit_hit(rect, o))
                && self.edit_begin_drag(i)
            {
                out.taken = true;
                out.busy = true;
                if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                    self.edit_drag(rect, p, step);
                }
            }
        } else if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(m) = mouse
        {
            self.edit_click(rect, m);
            out.taken = true;
        }
        out
    }

    // ----- painting -----

    /// The grid, the sketch so far, the preview of what will be made; or the outline being edited.
    pub fn sketch_paint(&self, p: &Painter, rect: Rect) {
        if let Some(e) = &self.sk.edit {
            self.paint_edit(p, rect, e);
            return;
        }
        if !self.sk.on {
            return;
        }
        let f = self.sk.frame;
        self.paint_grid(p, rect, &f);
        let st = &self.sk;
        match st.stage {
            Stage::Ready => self.paint_ready(p, rect),
            Stage::LoftTop | Stage::SweepPath => {
                self.paint_outline(p, rect, &st.pframe, &st.profile, Color32::from_white_alpha(110), 1.3);
                self.paint_saved_path(p, rect);
                self.paint_draft(p, rect);
            }
            Stage::Draw => {
                self.paint_saved_path(p, rect);
                self.paint_draft(p, rect);
            }
        }
    }

    /// A polyline on a frame with a dark halo, so it reads on any background.
    fn paint_outline(&self, p: &Painter, rect: Rect, f: &Frame, pts: &[[f32; 2]], color: Color32, w: f32) {
        self.paint_poly(p, rect, f, pts, true, color, w);
    }

    // Keep the coordinate frame, closure and stroke styling explicit at sketch call sites.
    #[allow(clippy::too_many_arguments)]
    fn paint_poly(&self, p: &Painter, rect: Rect, f: &Frame, pts: &[[f32; 2]], closed: bool, color: Color32, w: f32) {
        let px: Vec<Pos2> = pts.iter().filter_map(|q| self.sketch_px(rect, f, *q)).collect();
        halo_line(p, px, closed && pts.len() >= 3, color, w);
    }

    fn paint_world_poly(&self, p: &Painter, rect: Rect, pts: &[V3], closed: bool, color: Color32, w: f32) {
        let px: Vec<Pos2> = pts.iter().filter_map(|q| self.cam.project(rect, *q)).collect();
        halo_line(p, px, closed, color, w);
    }

    fn paint_saved_path(&self, p: &Painter, rect: Rect) {
        if self.sk.path.len() >= 2 {
            self.paint_world_poly(p, rect, &self.sk.path, false, style::ACCENT.gamma_multiply(0.55), 1.4);
        }
    }

    /// Grid lines on the plane, spaced to the zoom, covering what the camera sees.
    fn paint_grid(&self, p: &Painter, rect: Rect, f: &Frame) {
        // where the corners and the middle of the view fall on the plane
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        let mut hits = 0;
        for (fx, fy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.5, 0.5)] {
            let s = pos2(rect.left() + rect.width() * fx, rect.top() + rect.height() * fy);
            let (o, d) = self.cam.ray_at(rect, s);
            if let Some(q) = f.hit(o, d) {
                for i in 0..2 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                }
                hits += 1;
            }
        }
        let centre = f.from_world(self.cam.target);
        if hits < 3 {
            for i in 0..2 {
                lo[i] = centre[i] - 150.0;
                hi[i] = centre[i] + 150.0;
            }
        }
        // never more than a screenful around the view's middle
        for i in 0..2 {
            lo[i] = lo[i].max(centre[i] - 800.0).max(-sketch::COORD_MAX);
            hi[i] = hi[i].min(centre[i] + 800.0).min(sketch::COORD_MAX);
        }
        let mm_px = self.cam.mm_per_px(f.to_world(centre), rect.height()).max(1e-4);
        let mut step = nice_step(mm_px * 26.0);
        while ((hi[0] - lo[0]) / step > 90.0 || (hi[1] - lo[1]) / step > 90.0) && step < 1e5 {
            step *= 2.0;
        }
        let major = step * 5.0;
        let line = |a: [f32; 2], b: [f32; 2], c: Color32, w: f32| {
            if let (Some(a), Some(b)) = (self.sketch_px(rect, f, a), self.sketch_px(rect, f, b)) {
                p.line_segment([a, b], Stroke::new(w, c));
            }
        };
        let mut x = (lo[0] / step).ceil() * step;
        let mut guard = 0;
        while x <= hi[0] && guard < 200 {
            let (c, w) = grid_look(x, major);
            line([x, lo[1]], [x, hi[1]], c, w);
            x += step;
            guard += 1;
        }
        let mut y = (lo[1] / step).ceil() * step;
        guard = 0;
        while y <= hi[1] && guard < 200 {
            let (c, w) = grid_look(y, major);
            line([lo[0], y], [hi[0], y], c, w);
            y += step;
            guard += 1;
        }
        // the axis a revolve turns about
        if f.is_vertical() && lo[0] <= 0.0 && hi[0] >= 0.0 {
            line([0.0, lo[1]], [0.0, hi[1]], style::ACCENT.gamma_multiply(0.5), 1.4);
        }
    }

    /// The gesture in progress: points, rubber band, size labels.
    fn paint_draft(&self, p: &Painter, rect: Rect) {
        let st = &self.sk;
        let f = &st.frame;
        let pen = st.active_pen();
        let cur = st.cursor;
        let color = style::ACCENT;
        match pen {
            Pen::Polygon | Pen::Spline | Pen::Line => {
                let mut ctrl = st.pts.clone();
                let bad = cur.is_some_and(|c| st.would_cross(c));
                if let Some(c) = cur {
                    ctrl.push(c);
                }
                let shown: Vec<[f32; 2]> = match (pen, st.smooth) {
                    (Pen::Spline, true) if ctrl.len() >= 3 => sketch::spline_closed(&ctrl, MAX_OUTLINE),
                    (Pen::Line, true) if ctrl.len() >= 3 => sketch::spline_open(&ctrl, MAX_PATH),
                    _ => ctrl.clone(),
                };
                let closed = pen == Pen::Spline && st.smooth && ctrl.len() >= 3;
                let col = if bad { style::CUT } else { color };
                self.paint_poly(p, rect, f, &shown, closed, col, 1.7);
                // the way back to the first point
                if pen != Pen::Line
                    && !closed
                    && st.pts.len() >= 3
                    && let (Some(a), Some(b)) =
                        (st.pts.last().and_then(|q| self.sketch_px(rect, f, *q)), st.pts.first().and_then(|q| self.sketch_px(rect, f, *q)))
                {
                    p.line_segment([a, b], Stroke::new(1.0, color.gamma_multiply(0.35)));
                }
                for (i, q) in st.pts.iter().enumerate() {
                    if let Some(c) = self.sketch_px(rect, f, *q) {
                        let first = i == 0 && pen != Pen::Line && st.pts.len() >= 3;
                        let hot = first && st.mouse.is_some_and(|m| (m - c).length() <= CLOSE_PX);
                        point_dot(p, c, if hot { 7.0 } else { 4.0 }, hot || i == 0);
                    }
                }
                if let (Some(c), Some(last)) = (cur, st.pts.last())
                    && let Some(m) = self.sketch_px(rect, f, c)
                {
                    let l = (c[0] - last[0]).hypot(c[1] - last[1]);
                    tag(p, m + vec2(14.0, 12.0), &format!("{} mm", fmt(l)));
                }
            }
            Pen::Rect | Pen::Circle => {
                if let (Some(a), Some(c)) = (st.anchor, cur) {
                    let shape = if pen == Pen::Rect {
                        sketch::rect(a, c)
                    } else {
                        sketch::circle(a, (a[0] - c[0]).hypot(a[1] - c[1]), sketch::CIRCLE_POINTS)
                    };
                    self.paint_poly(p, rect, f, &shape, true, color, 1.7);
                    if let (Some(pa), Some(pc)) = (self.sketch_px(rect, f, a), self.sketch_px(rect, f, c)) {
                        point_dot(p, pa, 4.0, true);
                        if pen == Pen::Circle {
                            p.line_segment([pa, pc], Stroke::new(1.0, color.gamma_multiply(0.6)));
                        }
                        let text = if pen == Pen::Rect {
                            format!("{} × {} mm", fmt((a[0] - c[0]).abs()), fmt((a[1] - c[1]).abs()))
                        } else {
                            format!("半径 {} mm", fmt((a[0] - c[0]).hypot(a[1] - c[1])))
                        };
                        tag(p, pc + vec2(14.0, 12.0), &text);
                    }
                }
            }
        }
        if let Some(c) = cur
            && let Some(m) = self.sketch_px(rect, f, c)
        {
            p.circle_stroke(m, 5.0, Stroke::new(1.3, style::ACCENT));
            if st.pts.is_empty() && st.anchor.is_none() {
                tag(p, m + vec2(14.0, 12.0), &format!("{}, {}", fmt(c[0]), fmt(c[1])));
            }
        }
    }

    /// The finished profile and a wire sketch of what the card is set to make.
    fn paint_ready(&self, p: &Painter, rect: Rect) {
        let st = &self.sk;
        let f = &st.pframe;
        let ghost = style::ACCENT.gamma_multiply(0.55);
        self.paint_outline(p, rect, f, &st.profile, style::ACCENT, 2.0);
        let n = f.normal();
        let up = |h: f32| crate::sdf::mul(n, if st.flip { -h } else { h });
        let h = if st.height.is_finite() { st.height.clamp(0.1, 2000.0) } else { 10.0 };
        // a few vertical edges between two outlines of the same length
        let rails = |a: &[[f32; 2]], b: &[[f32; 2]], lift: V3| {
            if a.len() != b.len() || a.is_empty() {
                return;
            }
            let every = a.len().div_ceil(12).max(1);
            for i in (0..a.len()).step_by(every) {
                let (q0, q1) = (f.to_world(a[i]), crate::sdf::add(f.to_world(b[i]), lift));
                if let (Some(x), Some(y)) = (self.cam.project(rect, q0), self.cam.project(rect, q1)) {
                    p.line_segment([x, y], Stroke::new(1.0, ghost));
                }
            }
        };
        match st.make {
            Make::Extrude => {
                let top: Vec<V3> = st.profile.iter().map(|q| crate::sdf::add(f.to_world(*q), up(h))).collect();
                self.paint_world_poly(p, rect, &top, true, ghost, 1.3);
                rails(&st.profile, &st.profile, up(h));
            }
            Make::Loft => {
                let top2 = match st.loft_top {
                    LoftTop::Scaled => sketch::scaled(&st.profile, st.loft_scale),
                    LoftTop::Drawn => st.profile2.clone(),
                };
                let top: Vec<V3> = top2.iter().map(|q| crate::sdf::add(f.to_world(*q), up(h))).collect();
                self.paint_world_poly(p, rect, &top, true, ghost, 1.3);
                rails(&st.profile, &top2, up(h));
            }
            Make::Revolve => {
                // rings through the corners of the profile, about the vertical axis
                let every = st.profile.len().div_ceil(8).max(1);
                for i in (0..st.profile.len()).step_by(every) {
                    let q = st.profile[i];
                    let w = f.to_world(q);
                    let r = q[0].max(0.0);
                    let ring: Vec<V3> = (0..=40)
                        .map(|k| {
                            let a = std::f32::consts::TAU * k as f32 / 40.0;
                            [r * a.cos(), r * a.sin(), w[2]]
                        })
                        .collect();
                    self.paint_world_poly(p, rect, &ring, false, ghost.gamma_multiply(0.8), 1.0);
                }
            }
            Make::Sweep => self.paint_saved_path(p, rect),
        }
    }

    fn paint_edit(&self, p: &Painter, rect: Rect, e: &Edit) {
        let Some(sh) = self.scene.shape(e.g, e.s) else { return };
        let Some(f) = sketch::edit_frame(sh, e.which) else { return };
        self.paint_grid(p, rect, &f);
        // the loft's other profile, faintly
        if sh.kind == Kind::Loft {
            let other = if e.which == Which::Main { Which::Top } else { Which::Main };
            if let Some(of) = sketch::edit_frame(sh, other) {
                self.paint_poly(p, rect, &of, sketch::outline_of(sh, other), true, Color32::from_white_alpha(70), 1.2);
            }
        }
        let ol = sketch::outline_of(sh, e.which);
        self.paint_poly(p, rect, &f, ol, true, style::ACCENT, 2.0);
        for (i, q) in ol.iter().enumerate() {
            let Some(c) = self.sketch_px(rect, &f, *q) else { continue };
            let hot = e.hover == Some(Hover::Point(i)) || e.drag.as_ref().is_some_and(|d| d.idx == i);
            let chosen = e.sel == Some(i);
            point_dot(
                p,
                c,
                if hot {
                    7.0
                } else if chosen {
                    6.0
                } else {
                    4.5
                },
                hot || chosen,
            );
            if chosen {
                tag(p, c + vec2(12.0, 10.0), &format!("{}, {}", fmt(q[0]), fmt(q[1])));
            }
        }
        if let Some(Hover::Edge(_, at)) = e.hover
            && let Some(c) = self.sketch_px(rect, &f, at)
        {
            p.circle_stroke(c, 6.0, Stroke::new(1.4, style::ACCENT));
            p.line_segment([c - vec2(3.0, 0.0), c + vec2(3.0, 0.0)], Stroke::new(1.4, style::ACCENT));
            p.line_segment([c - vec2(0.0, 3.0), c + vec2(0.0, 3.0)], Stroke::new(1.4, style::ACCENT));
        }
    }

    // ----- the card -----

    /// The floating card (under the top of the viewport) while the tool is on or an outline is edited.
    pub fn sketch_card(&mut self, ctx: &egui::Context) {
        if !self.sketch_active() {
            return;
        }
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("sketch_card"))
            .order(egui::Order::Middle)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(pos2(rect.center().x, rect.top() + 12.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_max_width(440.0);
                    if self.sk.edit.is_some() {
                        self.edit_card(ui);
                    } else {
                        self.draw_card_body(ui, ctx);
                    }
                });
            });
    }

    fn edit_card(&mut self, ui: &mut Ui) {
        let mut done = false;
        let loft = self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s)).is_some_and(|s| s.kind == Kind::Loft);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("スケッチを編集").strong().color(style::ACCENT));
            if style::pill(ui, "おわり", false).on_hover_text("編集を終える (Enter / Esc)").clicked() {
                done = true;
            }
        });
        if loft {
            let cur = self.sk.edit.as_ref().map(|e| e.which);
            ui.horizontal(|ui| {
                for (w, label) in [(Which::Main, "下の輪かく"), (Which::Top, "上の輪かく")] {
                    if style::pill(ui, label, cur == Some(w)).clicked()
                        && let Some(e) = self.sk.edit.as_mut()
                    {
                        e.which = w;
                        e.sel = None;
                        e.drag = None;
                        let frame = self.scene.shape(e.g, e.s).and_then(|s| sketch::edit_frame(s, w));
                        if let Some(f) = frame {
                            self.sketch_fly(&f);
                        }
                    }
                }
            });
        }
        let n = self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s).map(|s| sketch::outline_of(s, e.which).len())).unwrap_or(0);
        hint(ui, &format!("点をドラッグして動かす　線をクリックで点をふやす　点をクリックして Delete で消す　{n}/{MAX_OUTLINE}こ"));
        hint(ui, "1mmきざみ　Shift = 10mm　Ctrl = 0.1mm　Alt+ドラッグで見る向きを回す");
        if !self.sk.msg.is_empty() {
            ui.label(egui::RichText::new(&self.sk.msg).size(11.5).color(style::CUT));
        }
        if done {
            self.sk.edit = None;
            self.sk.msg.clear();
        }
    }

    fn draw_card_body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut off = false;
        let stage = self.sk.stage;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("スケッチ").strong().color(style::ACCENT));
            let sub = match stage {
                Stage::Draw => "平面に絵をかく",
                Stage::Ready => "できた輪かくを、立体にします",
                Stage::LoftTop => "ロフトの上の輪かくをかく",
                Stage::SweepPath => "スイープの通り道 (線) をかく",
            };
            ui.label(egui::RichText::new(sub).size(11.0).color(style::DIM));
            if style::pill(ui, "やめる", false).on_hover_text("スケッチを終える (Esc)").clicked() {
                off = true;
            }
        });
        // the plane
        let plane_free = matches!(stage, Stage::Draw | Stage::SweepPath);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("面").size(11.0).color(style::DIM));
            for pl in PlaneSel::ALL {
                let usable = plane_free && (pl != PlaneSel::OnShape || self.sel.is_some());
                if style::pill_enabled(ui, pl.label(), self.sk.plane == pl, usable).on_hover_text(pl.tip()).clicked() {
                    self.sketch_set_plane(pl);
                }
            }
            if plane_free && style::pill(ui, "見る", false).on_hover_text("この面を正面から見る").clicked() {
                let f = self.sk.frame;
                self.sketch_fly(&f);
            }
        });
        // the pens
        if matches!(stage, Stage::Draw | Stage::LoftTop) {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("道具").size(11.0).color(style::DIM));
                for pen in Pen::ALL {
                    let usable = !(stage == Stage::LoftTop && pen == Pen::Line);
                    if style::pill_enabled(ui, pen.label(), self.sk.pen == pen, usable).on_hover_text(pen.tip()).clicked() {
                        self.sk.pen = pen;
                        self.sk.clear_drawing();
                    }
                }
            });
        }
        if matches!(stage, Stage::Draw | Stage::LoftTop | Stage::SweepPath) {
            let pen = self.sk.active_pen();
            if matches!(pen, Pen::Spline | Pen::Line) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("かたち").size(11.0).color(style::DIM));
                    if style::pill(ui, "なめらか", self.sk.smooth).on_hover_text("点を通る、なめらかな曲線").clicked() {
                        self.sk.smooth = true;
                    }
                    if style::pill(ui, "かくばる", !self.sk.smooth).on_hover_text("点と点をまっすぐつなぐ").clicked() {
                        self.sk.smooth = false;
                    }
                });
            }
            hint(ui, pen_hint(pen, stage));
            hint(ui, "1mmきざみ　Shift = 10mm　Ctrl = 0.1mm　Backspaceで1つもどる　Escでやりなおし");
        }
        if stage == Stage::Draw && !self.sk.path.is_empty() {
            hint(ui, &format!("おぼえている線: {}点（スイープで使えます）", self.sk.path.len()));
        }
        if stage == Stage::Ready {
            self.make_card(ui, ctx);
        }
        if !self.sk.msg.is_empty() {
            ui.label(egui::RichText::new(&self.sk.msg).size(11.5).color(style::CUT));
        }
        if off {
            self.set_sketch_tool(false);
        }
    }

    /// The part of the card that asks what to make.
    fn make_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let vertical = self.sk.pframe.is_vertical();
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("つくる").size(11.0).color(style::DIM));
            for m in Make::ALL {
                let usable = m != Make::Revolve || vertical;
                let tip = match m {
                    Make::Extrude => "輪かくをまっすぐ押し出して、柱にする",
                    Make::Revolve if vertical => "輪かくを、たてのじく (中心の線) のまわりに回して、つぼのような形にする",
                    Make::Revolve => "回転体は「正面」か「横」の面でかいたときに使えます",
                    Make::Loft => "下の輪かくから上の輪かくへ、なめらかにつなぐ",
                    Make::Sweep => "輪かくを、かいた線にそって引っぱる",
                };
                if style::pill_enabled(ui, m.label(), self.sk.make == m, usable).on_hover_text(tip).clicked() {
                    self.sk.make = m;
                    self.sk.msg.clear();
                }
            }
        });
        let make = self.sk.make;
        if matches!(make, Make::Extrude | Make::Loft) {
            style::labeled_slider(ui, "高さ", &mut self.sk.height, 0.5..=300.0, "押し出す高さ (mm)");
            ui.horizontal(|ui| {
                if style::pill(ui, "うらがわへ", self.sk.flip).on_hover_text("面の反対がわへ伸ばす").clicked() {
                    self.sk.flip = !self.sk.flip;
                }
            });
        }
        if make == Make::Loft {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("上の輪かく").size(11.0).color(style::DIM));
                if style::pill(ui, "コピーを拡大・縮小", self.sk.loft_top == LoftTop::Scaled).clicked() {
                    self.sk.loft_top = LoftTop::Scaled;
                }
                if style::pill(ui, "もう1つかく", self.sk.loft_top == LoftTop::Drawn).clicked() {
                    self.sk.loft_top = LoftTop::Drawn;
                    if self.sk.profile2.is_empty() {
                        self.sk.stage = Stage::LoftTop;
                        self.sk.clear_drawing();
                    }
                }
            });
            if self.sk.loft_top == LoftTop::Scaled {
                style::labeled_slider(ui, "上の大きさ", &mut self.sk.loft_scale, 0.1..=3.0, "上の輪かくの大きさ (下を1とした倍率)");
            } else if style::pill(ui, if self.sk.profile2.is_empty() { "上の輪かくをかく" } else { "上の輪かくをかきなおす" }, false).clicked()
            {
                self.sk.stage = Stage::LoftTop;
                self.sk.profile2.clear();
                self.sk.clear_drawing();
            }
        }
        if make == Make::Sweep {
            style::labeled_slider(ui, "ふとさ", &mut self.sk.sweep_scale, 0.2..=4.0, "輪かくの大きさの倍率");
            ui.horizontal(|ui| {
                let have = !self.sk.path.is_empty();
                ui.label(
                    egui::RichText::new(if have { format!("線: {}点", self.sk.path.len()) } else { "線がまだありません".to_string() })
                        .size(11.5)
                        .color(style::DIM),
                );
                if style::pill(ui, if have { "線をかきなおす" } else { "線をかく" }, false).on_hover_text("通り道をかく (Enterで終わり)").clicked()
                {
                    self.sk.stage = Stage::SweepPath;
                    self.sk.clear_drawing();
                    self.sk.frame = self.sketch_plane();
                    let f = self.sk.frame;
                    self.sketch_fly(&f);
                }
            });
        }
        ui.horizontal(|ui| {
            let ready = match make {
                Make::Sweep => !self.sk.path.is_empty(),
                Make::Loft => self.sk.loft_top == LoftTop::Scaled || !self.sk.profile2.is_empty(),
                _ => true,
            };
            let b = egui::Button::new(egui::RichText::new(format!("{}を作る", make.label())).strong().color(Color32::WHITE))
                .fill(style::ACCENT_SOFT)
                .stroke(Stroke::new(1.0, style::ACCENT));
            if ui.add_enabled(ready, b).clicked() {
                self.sketch_make(ctx);
            }
            if style::pill(ui, "かきなおす", false).on_hover_text("輪かくをすてて、もう一度かく (Esc)").clicked() {
                self.sk.reset();
            }
        });
    }
}

fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(11.0).color(style::DIM)).wrap());
}

fn pen_hint(pen: Pen, stage: Stage) -> &'static str {
    match (pen, stage) {
        (_, Stage::LoftTop) if pen != Pen::Line => "ロフトの上の輪かくをかきます (下の輪かくはうすく見えています)",
        (Pen::Polygon, _) => "クリックで角をうつ　さいしょの点 / Enter / ダブルクリックでとじる",
        (Pen::Rect, _) => "ドラッグ (または2回クリック) で、向かい合う2つの角をきめる",
        (Pen::Circle, _) => "まん中から外へドラッグ (または2回クリック) で半径をきめる",
        (Pen::Spline, _) => "クリックした点を通る、なめらかな輪かく　さいしょの点 / Enterでとじる",
        (Pen::Line, Stage::SweepPath) => "スイープの通り道をかきます　Enter / ダブルクリックでおわり",
        (Pen::Line, _) => "クリックで点をうつ　Enter / ダブルクリックでおわり (スイープの通り道になります)",
    }
}

// ------------------------------------------------------------------ painting helpers

fn nice_step(raw: f32) -> f32 {
    let raw = if raw.is_finite() { raw.max(1e-3) } else { 1.0 };
    for s in [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0] {
        if s >= raw {
            return s;
        }
    }
    1000.0
}

fn grid_look(v: f32, major: f32) -> (Color32, f32) {
    if v.abs() < 1e-4 {
        (Color32::from_white_alpha(70), 1.3)
    } else if ((v / major).round() * major - v).abs() < 1e-3 {
        (Color32::from_white_alpha(30), 1.0)
    } else {
        (Color32::from_white_alpha(14), 0.8)
    }
}

/// "12" or "12.5" (no more than one decimal).
fn fmt(v: f32) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-4 { format!("{}", r.round() as i64) } else { format!("{r:.1}") }
}

/// A line with a dark halo.
fn halo_line(p: &Painter, px: Vec<Pos2>, closed: bool, color: Color32, w: f32) {
    if px.len() < 2 {
        return;
    }
    let halo = Stroke::new(w + 2.4, Color32::from_black_alpha(90));
    let ink = Stroke::new(w, color);
    if closed && px.len() >= 3 {
        p.add(Gfx::closed_line(px.clone(), halo));
        p.add(Gfx::closed_line(px, ink));
    } else {
        p.add(Gfx::line(px.clone(), halo));
        p.add(Gfx::line(px, ink));
    }
}

fn point_dot(p: &Painter, c: Pos2, r: f32, lit: bool) {
    p.circle_filled(c, r + 1.2, Color32::from_black_alpha(110));
    p.circle_filled(c, r, if lit { Color32::WHITE } else { style::PANEL_DARK });
    p.circle_stroke(c, r, Stroke::new(1.5, style::ACCENT));
}

/// A small label with a dark plate.
fn tag(p: &Painter, at: Pos2, text: &str) {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(11.5), style::TEXT);
    let r = Rect::from_min_size(at, g.size()).expand2(vec2(5.0, 2.0));
    p.rect_filled(r, 4.0, Color32::from_black_alpha(160));
    p.galley(at, g, style::TEXT);
}

/// The icon of the sketch tool: an outline with its corner points, and a pencil.
pub fn glyph(p: &Painter, r: Rect, ink: Color32) {
    let s = r.width().min(r.height()) * 0.8;
    let f = Rect::from_center_size(r.center(), vec2(s, s));
    let at = |u: f32, v: f32| pos2(f.left() + f.width() * u, f.top() + f.height() * v);
    let pen = Stroke::new((r.width() * 0.075).clamp(1.2, 2.6), ink);
    let poly = [at(0.08, 0.60), at(0.20, 0.14), at(0.62, 0.10), at(0.70, 0.40), at(0.40, 0.52)];
    p.add(Gfx::closed_line(poly.to_vec(), pen));
    for q in poly {
        p.circle_filled(q, (r.width() * 0.05).clamp(1.4, 2.4), ink);
    }
    // a pencil pointing to the lower left
    let tip = at(0.42, 0.92);
    let (a, b) = (at(0.50, 0.68), at(0.66, 0.84));
    p.add(Gfx::closed_line(vec![tip, a, b], pen));
    p.line_segment([a, at(0.80, 0.40)], pen);
    p.line_segment([b, at(0.96, 0.56)], pen);
    p.line_segment([at(0.80, 0.40), at(0.96, 0.56)], pen);
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::model::Scene;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 500.0))
    }

    fn app() -> App {
        let mut app = App::new(true, None);
        app.view_rect = rect();
        app
    }

    fn profile_of(f: Option<Finished>) -> Vec<[f32; 2]> {
        match f {
            Some(Finished::Profile(p)) => p,
            other => panic!("a profile was expected: {other:?}"),
        }
    }

    fn click_all(st: &mut State, pts: &[[f32; 2]]) {
        for p in pts {
            assert!(st.click(*p, false).is_none());
        }
    }

    #[test]
    fn a_polygon_is_closed_by_clicking_the_first_point_or_enter() {
        let mut st = State { pen: Pen::Polygon, ..State::default() };
        click_all(&mut st, &[[0.0, 0.0], [20.0, 0.0], [20.0, 10.0]]);
        let p = profile_of(st.click([0.0, 10.0], false).or_else(|| st.click([0.0, 0.0], true)));
        // the fourth point was only added; clicking the first one closed it
        assert_eq!(p.len(), 4);
        assert!(sketch::signed_area(&p) > 0.0);
        assert!(st.pts.is_empty(), "the next outline starts empty");
        // Enter closes too, and fewer than three points do not
        click_all(&mut st, &[[0.0, 0.0], [20.0, 0.0]]);
        assert!(st.enter().is_none() && !st.msg.is_empty(), "two points: a friendly message");
        assert_eq!(st.pts.len(), 2, "the points stay so they can be fixed");
        st.pts.push([10.0, 15.0]);
        assert_eq!(profile_of(st.enter()).len(), 3);
        // a clockwise outline comes out counter-clockwise
        click_all(&mut st, &[[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]]);
        assert!(sketch::signed_area(&profile_of(st.enter())) > 0.0);
    }

    #[test]
    fn a_point_that_would_cross_the_outline_is_refused() {
        let mut st = State { pen: Pen::Polygon, ..State::default() };
        click_all(&mut st, &[[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [10.0, 20.0]]);
        assert!(st.would_cross([10.0, -10.0]));
        assert!(!st.would_cross([0.0, 20.0]));
        assert!(st.click([10.0, -10.0], false).is_none());
        assert_eq!(st.pts.len(), 4, "refused");
        assert!(!st.msg.is_empty());
        // the same point twice is not added twice
        st.click([0.0, 20.0], false);
        st.click([0.0, 20.0], false);
        assert_eq!(st.pts.len(), 5);
        // a bow tie can never be finished
        let mut bow = State { pts: vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 15.0]], ..State::default() };
        assert!(bow.enter().is_none());
        assert_eq!(bow.msg, SketchError::SelfIntersect.message());
    }

    #[test]
    fn rectangles_and_circles_are_dragged_or_clicked_out() {
        let mut st = State { pen: Pen::Rect, ..State::default() };
        st.press([5.0, 5.0]);
        assert!(st.dragging && st.anchor == Some([5.0, 5.0]));
        let p = profile_of(st.release([25.0, 15.0]));
        assert_eq!(p.len(), 4);
        assert_eq!(sketch::bbox(&p), Some(([5.0, 5.0], [25.0, 15.0])));
        assert!(st.anchor.is_none());
        // two clicks
        assert!(st.click([0.0, 0.0], false).is_none() && st.anchor.is_some());
        assert_eq!(profile_of(st.click([8.0, 3.0], false)).len(), 4);
        // a drag that goes nowhere keeps the corner for the second click
        st.press([1.0, 1.0]);
        assert!(st.release([1.0, 1.0]).is_none() && st.anchor == Some([1.0, 1.0]));
        // a flat rectangle is refused
        assert!(st.click([30.0, 1.0], false).is_none());
        assert!(!st.msg.is_empty());
        // circles have 32 points
        st.pen = Pen::Circle;
        st.clear_drawing();
        st.press([0.0, 0.0]);
        let c = profile_of(st.release([10.0, 0.0]));
        assert_eq!(c.len(), sketch::CIRCLE_POINTS);
        assert!(c.iter().all(|q| ((q[0]).hypot(q[1]) - 10.0).abs() < 1e-3));
        // the pens that click do not drag
        st.pen = Pen::Polygon;
        st.press([0.0, 0.0]);
        assert!(!st.dragging);
    }

    #[test]
    fn a_spline_is_smooth_within_the_limit_and_corners_can_be_chosen() {
        let mut st = State { pen: Pen::Spline, smooth: true, ..State::default() };
        click_all(&mut st, &[[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]]);
        let p = profile_of(st.enter());
        assert!(p.len() > 8 && p.len() <= MAX_OUTLINE, "{} points", p.len());
        st.smooth = false;
        click_all(&mut st, &[[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]]);
        assert_eq!(profile_of(st.enter()).len(), 4);
        // never more clicks than the outline can take
        st.smooth = true;
        for i in 0..200 {
            let a = std::f32::consts::TAU * i as f32 / 200.0;
            st.click([(50.0 * a.cos()).round(), (50.0 * a.sin()).round()], false);
        }
        assert!(st.pts.len() <= MAX_OUTLINE);
        assert!(st.msg.contains(&MAX_OUTLINE.to_string()));
    }

    #[test]
    fn a_line_is_kept_as_a_path_and_stops_at_the_node_limit() {
        let mut app = app();
        app.sk.on = true;
        app.sk.pen = Pen::Line;
        app.sk.smooth = true;
        click_all(&mut app.sk, &[[0.0, 0.0], [20.0, 10.0], [40.0, -10.0], [60.0, 0.0]]);
        let l = app.sk.enter();
        let Some(Finished::Line(line)) = l.clone() else { panic!("a line: {l:?}") };
        assert!(line.len() > 4 && line.len() <= MAX_PATH);
        app.sketch_done(Finished::Line(line));
        assert_eq!(app.sk.stage, Stage::Draw);
        assert!(app.sk.path.len() > 4 && app.sk.path.iter().all(|p| p[2] == 0.0), "on the floor");
        // a single point is no line
        app.sk.click([5.0, 5.0], false);
        assert!(app.sk.enter().is_none() && !app.sk.msg.is_empty());
        app.sk.clear_drawing();
        for i in 0..100 {
            app.sk.click([i as f32 * 3.0, (i % 5) as f32 * 4.0], false);
        }
        assert!(app.sk.pts.len() <= MAX_PATH);
    }

    #[test]
    fn backspace_and_escape_step_back() {
        let mut st = State { pen: Pen::Polygon, ..State::default() };
        click_all(&mut st, &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        st.back();
        assert_eq!(st.pts.len(), 2);
        assert!(!st.escape() && st.pts.is_empty(), "first Esc drops the points");
        assert!(st.escape(), "an empty page: the tool should end");
        st.stage = Stage::Ready;
        st.profile = sketch::rect([0.0, 0.0], [5.0, 5.0]);
        assert!(!st.escape() && st.stage == Stage::Draw && st.profile.is_empty());
        st.stage = Stage::LoftTop;
        assert!(!st.escape() && st.stage == Stage::Ready);
        st.pen = Pen::Rect;
        st.click([1.0, 1.0], false);
        st.back();
        assert!(st.anchor.is_none());
    }

    /// Draw a profile on the floor and make every kind of solid of it, through the same calls the card uses.
    #[test]
    fn a_profile_becomes_each_kind_of_solid_in_the_right_group() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        assert!(app.sk.on && app.fly.is_some(), "the camera glides to look at the plane");
        let n = app.scene.shape_count();
        app.sk.pen = Pen::Rect;
        app.sk.click([10.0, 10.0], false);
        let p = app.sk.click([40.0, 30.0], false);
        app.sketch_done(p.expect("a profile"));
        assert_eq!(app.sk.stage, Stage::Ready);
        // 押し出し
        app.sk.make = Make::Extrude;
        app.sk.height = 12.0;
        assert!(app.sketch_make(&ctx));
        assert_eq!(app.scene.shape_count(), n + 1);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let sh = app.scene.shape(g, s).expect("there");
        assert_eq!((sh.kind, sh.size[0], sh.pos), (Kind::Extrude, 12.0, [25.0, 20.0, 6.0]));
        assert_eq!(sh.outline.len(), 4);
        assert_eq!(app.sk.stage, Stage::Draw, "ready for the next sketch");
        // a vertical plane: front, with the camera looking at it
        app.sketch_set_plane(PlaneSel::Front);
        assert_eq!(app.sk.frame, Frame::front());
        app.sk.pen = Pen::Polygon;
        click_all(&mut app.sk, &[[10.0, 0.0], [25.0, 0.0], [20.0, 40.0], [10.0, 40.0]]);
        let p = app.sk.enter().expect("closed");
        app.sketch_done(p);
        app.sk.make = Make::Revolve;
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let rev = app.scene.shape(g, s).expect("there");
        assert_eq!(rev.kind, Kind::Revolve);
        assert_eq!(rev.params[0], 360.0);
        assert!(rev.outline.iter().all(|q| q[0] >= 0.0));
        // a loft with a scaled copy
        app.sk.pen = Pen::Circle;
        app.sk.press([0.0, 0.0]);
        let c = app.sk.release([10.0, 0.0]).expect("a circle");
        app.sketch_done(c);
        app.sk.make = Make::Loft;
        app.sk.loft_top = LoftTop::Scaled;
        app.sk.loft_scale = 0.5;
        app.sk.height = 25.0;
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let l = app.scene.shape(g, s).expect("there");
        assert_eq!(l.kind, Kind::Loft);
        assert_eq!(l.outline.len(), l.outline2.len());
        let r0 = l.outline.iter().map(|q| q[0].hypot(q[1])).fold(0.0_f32, f32::max);
        let r1 = l.outline2.iter().map(|q| q[0].hypot(q[1])).fold(0.0_f32, f32::max);
        assert!((r1 / r0 - 0.5).abs() < 0.02, "top is half the size: {r1} vs {r0}");
        // and the file survives a round trip with all of them in
        let back = Scene::from_json(&app.scene.to_json()).expect("loads");
        assert_eq!(back.shape_count(), app.scene.shape_count());
    }

    #[test]
    fn a_sweep_needs_a_path_and_a_loft_with_a_drawn_top_goes_through_a_second_profile() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Circle;
        app.sk.press([0.0, 0.0]);
        let c = app.sk.release([6.0, 0.0]).expect("a circle");
        app.sketch_done(c);
        // no path yet: nothing is made
        app.sk.make = Make::Sweep;
        assert!(!app.sketch_make(&ctx));
        assert!(!app.sk.msg.is_empty());
        // draw the path on the front plane
        app.sk.stage = Stage::SweepPath;
        app.sketch_set_plane(PlaneSel::Front);
        assert_eq!(app.sk.active_pen(), Pen::Line, "a path is always a line");
        click_all(&mut app.sk, &[[0.0, 0.0], [0.0, 30.0], [30.0, 50.0]]);
        let l = app.sk.enter().expect("a line");
        app.sketch_done(l);
        assert_eq!((app.sk.stage, app.sk.make), (Stage::Ready, Make::Sweep));
        assert_eq!(app.sk.frame, app.sk.pframe, "back on the profile's plane");
        let n = app.scene.shape_count();
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let sw = app.scene.shape(g, s).expect("there");
        assert_eq!(sw.kind, Kind::Sweep);
        assert!(sw.path.len() >= 3 && sw.path.len() <= MAX_PATH && sw.path.iter().all(|p| p[3] == 1.0));
        assert_eq!(app.scene.shape_count(), n + 1);
        // the line is remembered for the next sweep
        assert!(!app.sk.path.is_empty());
        // a loft whose top is drawn next
        app.sketch_set_plane(PlaneSel::Floor);
        app.sk.pen = Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        let p = app.sk.click([20.0, 20.0], false).expect("rect");
        app.sketch_done(p);
        app.sk.make = Make::Loft;
        app.sk.loft_top = LoftTop::Drawn;
        app.sk.stage = Stage::LoftTop;
        assert_eq!(app.sk.frame, app.sk.pframe);
        app.sk.pen = Pen::Circle;
        app.sk.click([10.0, 10.0], false);
        let c = app.sk.click([15.0, 10.0], false).expect("circle");
        app.sketch_done(c);
        assert_eq!((app.sk.stage, app.sk.profile2.len()), (Stage::Ready, sketch::CIRCLE_POINTS));
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let l = app.scene.shape(g, s).expect("there");
        assert_eq!((l.kind, l.outline.len(), l.outline2.len()), (Kind::Loft, 4, sketch::CIRCLE_POINTS));
    }

    #[test]
    fn the_new_shape_stands_exactly_where_the_sketch_was() {
        let ctx = egui::Context::default();
        let mut app = app();
        // a plane through the top of the starting ball (radius 20 at z = 20)
        app.sel = Some(Pick::Shape(0, 0));
        app.set_sketch_tool(true);
        app.sketch_set_plane(PlaneSel::OnShape);
        assert!((app.sk.frame.origin[2] - 40.0).abs() < 1e-3, "{:?}", app.sk.frame);
        app.sk.pen = Pen::Rect;
        app.sk.click([-5.0, -5.0], false);
        let p = app.sk.click([5.0, 5.0], false).expect("rect");
        app.sketch_done(p);
        app.sk.height = 10.0;
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let sh = app.scene.shape(g, s).expect("there").clone();
        let field = crate::sdf::Field::new(&Scene {
            groups: vec![crate::model::Group { shapes: vec![sh], ..crate::model::Group::new("g") }],
            ..Scene::default()
        });
        assert!(field.distance([0.0, 0.0, 45.0]) < 0.0, "inside the block on top of the ball");
        assert!(field.distance([0.0, 0.0, 39.0]) > 0.0 && field.distance([0.0, 0.0, 51.0]) > 0.0);
        assert!(field.distance([4.0, 4.0, 41.0]) < 0.0 && field.distance([7.0, 0.0, 45.0]) > 0.0);
        // with nothing selected the "on the shape" plane is not offered
        app.sel = None;
        assert!(app.selection_top().is_none());
    }

    #[test]
    fn the_mouse_position_maps_onto_the_plane_and_snaps() {
        let mut app = app();
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        app.cam.target = [0.0, 0.0, 0.0];
        app.cam.dist = 100.0;
        let r = rect();
        let f = Frame::floor();
        let at = |q: [f32; 2]| app.cam.project(r, f.to_world(q)).expect("on screen");
        for q in [[10.0, 5.0], [-23.0, 17.0], [0.0, 0.0]] {
            let got = app.sketch_cursor(r, at(q), 1.0, &f).expect("hit");
            assert!((got[0] - q[0]).abs() < 0.6 && (got[1] - q[1]).abs() < 0.6, "{got:?} vs {q:?}");
        }
        // a nudge of a few tenths snaps to the same whole millimetre; 10 mm snapping is coarser
        let near = at([10.2, 5.2]);
        assert_eq!(app.sketch_cursor(r, near, 1.0, &f), Some([10.0, 5.0]));
        assert_eq!(app.sketch_cursor(r, near, 10.0, &f), Some([10.0, 10.0]));
        assert_eq!(step_for(egui::Modifiers::NONE), 1.0);
        assert_eq!(step_for(egui::Modifiers::SHIFT), 10.0);
        assert_eq!(step_for(egui::Modifiers::COMMAND), 0.1);
        // a plane seen edge-on cannot be drawn on
        app.cam.look(0.0, 0.0);
        assert!(app.sketch_cursor(r, r.center(), 1.0, &Frame::floor()).is_none());
        // the glide ends looking straight at each plane
        for (pl, fr) in [(PlaneSel::Front, Frame::front()), (PlaneSel::Side, Frame::side()), (PlaneSel::Floor, Frame::floor())] {
            app.sketch_set_plane(pl);
            for _ in 0..300 {
                app.step_fly(0.016);
            }
            let (_, _, fwd) = app.cam.basis();
            assert!(crate::sdf::dot(fwd, fr.normal()) < -0.99, "{pl:?}: the camera looks into the plane");
        }
    }

    /// Edit the outline of a made shape point by point, with undo.
    #[test]
    fn an_outline_is_edited_in_the_viewport_with_undo() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        let p = app.sk.click([20.0, 20.0], false).expect("rect");
        app.sketch_done(p);
        assert!(app.sketch_make(&ctx));
        app.settle(&ctx);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        app.set_sketch_tool(false);
        app.sketch_edit_begin(g, s);
        assert!(app.sketch_active() && app.sk.edit.is_some());
        // look straight down and find the corner (20, 20) on screen
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        app.cam.target = [10.0, 10.0, 0.0];
        app.cam.dist = 90.0;
        let r = rect();
        let corner_px = |app: &App, i: usize| {
            let sh = app.scene.shape(g, s).expect("there");
            let f = sketch::edit_frame(sh, Which::Main).expect("frame");
            app.cam.project(r, f.to_world(sh.outline[i])).expect("on screen")
        };
        let before = app.scene.shape(g, s).expect("there").outline.clone();
        let c2 = corner_px(&app, 2);
        assert_eq!(app.edit_hit(r, c2 + vec2(3.0, 2.0)), Some(Hover::Point(2)));
        assert_eq!(app.edit_hit(r, c2 + vec2(60.0, 60.0)), None);
        // drag corner 2 outwards by about 10 mm
        assert!(app.edit_begin_drag(2));
        let target = app.cam.project(r, [30.0, 30.0, 0.0]).expect("on screen");
        app.edit_drag(r, target, 1.0);
        let moved = app.scene.shape(g, s).expect("there").outline[2];
        assert!((moved[0] - before[2][0] - 10.0).abs() < 1.0 && (moved[1] - before[2][1] - 10.0).abs() < 1.0, "{moved:?}");
        app.edit_release();
        app.settle(&ctx);
        app.undo();
        assert_eq!(app.scene.shape(g, s).expect("there").outline, before);
        app.redo();
        assert_ne!(app.scene.shape(g, s).expect("there").outline, before);
        // dragging a corner across the opposite edge is refused when the mouse is let go
        let good = app.scene.shape(g, s).expect("there").outline.clone();
        assert!(app.edit_begin_drag(0));
        let across = app.cam.project(r, [40.0, 0.0, 0.0]).expect("on screen");
        app.edit_drag(r, across, 1.0);
        assert_ne!(app.scene.shape(g, s).expect("there").outline, good, "it follows the mouse while the button is down");
        app.edit_release();
        assert_eq!(app.scene.shape(g, s).expect("there").outline, good, "a crossing outline is put back when the mouse is let go");
        assert!(!app.sk.msg.is_empty());
        // a click on a line adds a point after it
        let outline = app.scene.shape(g, s).expect("there").outline.clone();
        let n = outline.len();
        let a = corner_px(&app, 0);
        let b = corner_px(&app, 1);
        let mid = a + (b - a) * 0.5;
        assert!(matches!(app.edit_hit(r, mid), Some(Hover::Edge(0, _))));
        app.edit_click(r, mid);
        assert_eq!(app.scene.shape(g, s).expect("there").outline.len(), n + 1);
        assert_eq!(app.sk.edit.as_ref().and_then(|e| e.sel), Some(1), "the new point is chosen");
        // Delete removes it again; the corners of a triangle stay
        app.sketch_edit_delete();
        assert_eq!(app.scene.shape(g, s).expect("there").outline.len(), n);
        for _ in 0..10 {
            if let Some(e) = app.sk.edit.as_mut() {
                e.sel = Some(0);
            }
            app.sketch_edit_delete();
        }
        assert!(app.scene.shape(g, s).expect("there").outline.len() >= 3);
        assert!(!app.sk.msg.is_empty(), "told that three points stay");
        // a click on nothing chooses nothing
        app.edit_click(r, r.min + vec2(2.0, 2.0));
        assert_eq!(app.sk.edit.as_ref().and_then(|e| e.sel), None);
        // the shape deleted: the editing ends by itself
        app.delete_selected();
        assert!(app.edit_target().is_none() && app.sk.edit.is_none());
    }

    #[test]
    fn every_solid_kind_can_be_edited_and_other_shapes_cannot() {
        let mut app = app();
        // the default Sweep, Revolve, Loft, Extrude
        for k in [Kind::Extrude, Kind::Revolve, Kind::Loft, Kind::Sweep] {
            app.add_shape(&egui::Context::default(), k);
            let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
            app.sketch_edit_begin(g, s);
            assert!(app.sk.edit.is_some(), "{k:?}");
            // a revolve profile cannot be dragged to the left of the axis
            if k == Kind::Revolve {
                assert!(app.edit_begin_drag(1));
                app.cam = Camera::default();
                app.cam.look(0.0, 0.0);
                app.cam.target = [0.0, 0.0, 0.0];
                app.cam.dist = 120.0;
                let far_left = app.cam.project(rect(), [-60.0, 0.0, 10.0]).expect("on screen");
                app.edit_drag(rect(), far_left, 1.0);
                assert!(app.scene.shape(g, s).expect("there").outline.iter().all(|q| q[0] >= 0.0));
                app.edit_release();
            }
        }
        app.add_shape(&egui::Context::default(), Kind::Box);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        app.sketch_edit_begin(g, s);
        assert!(app.sk.edit.is_none() || app.sk.edit.as_ref().is_some_and(|e| (e.g, e.s) != (g, s)), "a box has no outline to edit");
    }

    #[test]
    fn the_tools_exclude_each_other_and_esc_ends_the_sketch() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_pipe_tool(true);
        app.set_sketch_tool(true);
        assert!(app.sk.on && !app.pt.on && !app.ct.drawing);
        app.set_pipe_tool(true);
        assert!(!app.sk.on, "starting the pipe tool ends the sketch");
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Polygon;
        app.sk.click([0.0, 0.0], false);
        app.sk.click([10.0, 0.0], false);
        let key = |k: egui::Key| {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE })
            })
        };
        key(egui::Key::Escape);
        app.sketch_keys(&ctx);
        assert!(app.sk.on && app.sk.pts.is_empty(), "the first Esc throws the points away");
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Escape);
        app.sketch_keys(&ctx);
        assert!(!app.sk.on, "Esc on an empty page ends the tool");
        // Backspace in the sketch tool does not delete the selected shape
        let n = app.scene.shape_count();
        app.set_sketch_tool(true);
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Backspace);
        app.sketch_keys(&ctx);
        assert_eq!(app.scene.shape_count(), n);
        // Enter closes the polygon
        app.sk.pen = Pen::Polygon;
        click_all(&mut app.sk, &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Enter);
        app.sketch_keys(&ctx);
        assert_eq!(app.sk.stage, Stage::Ready);
    }

    #[test]
    fn the_card_the_painting_and_the_icon_work_without_a_graphics_card() {
        let ctx = egui::Context::default();
        crate::fonts::setup(&ctx);
        style::apply(&ctx);
        let mut app = app();
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        app.cam.target = [20.0, 20.0, 0.0];
        app.cam.dist = 120.0;
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1380.0, 860.0));
        let frame = |app: &mut App| {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                app.sketch_card(ui.ctx());
                let painter = ui.painter().clone();
                app.sketch_paint(&painter, rect());
                egui::Area::new(egui::Id::new("t")).show(ui.ctx(), |ui| {
                    app.sketch_button(ui);
                });
            });
            assert!(!out.shapes.is_empty());
            // This headless test has no renderer to apply or release texture updates.
            out.textures_delta.clear();
        };
        // not active: nothing to show
        frame(&mut app);
        app.set_sketch_tool(true);
        for pen in Pen::ALL {
            app.sk.pen = pen;
            app.sk.clear_drawing();
            app.sk.cursor = Some([15.0, 12.0]);
            match pen {
                Pen::Rect | Pen::Circle => {
                    app.sk.anchor = Some([0.0, 0.0]);
                }
                _ => {
                    app.sk.pts = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 20.0]];
                }
            }
            frame(&mut app);
            app.sk.smooth = !app.sk.smooth;
            frame(&mut app);
        }
        // the finished profile with each preview
        app.sk.clear_drawing();
        app.sk.profile = sketch::rect([0.0, 0.0], [30.0, 20.0]);
        app.sk.pframe = Frame::floor();
        app.sk.stage = Stage::Ready;
        for m in Make::ALL {
            app.sk.make = m;
            app.sk.path = vec![[0.0; 3], [10.0, 0.0, 20.0], [30.0, 0.0, 30.0]];
            frame(&mut app);
        }
        app.sk.pframe = Frame::front();
        app.sk.make = Make::Revolve;
        frame(&mut app);
        app.sk.stage = Stage::LoftTop;
        frame(&mut app);
        app.sk.stage = Stage::SweepPath;
        frame(&mut app);
        app.sk.msg = "ためし".to_string();
        frame(&mut app);
        // editing
        app.set_sketch_tool(false);
        for k in [Kind::Extrude, Kind::Revolve, Kind::Loft, Kind::Sweep] {
            app.add_shape(&egui::Context::default(), k);
            let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
            app.sketch_edit_begin(g, s);
            if let Some(e) = app.sk.edit.as_mut() {
                e.sel = Some(1);
                e.hover = Some(Hover::Edge(0, [0.0, 0.0]));
            }
            frame(&mut app);
        }
    }

    #[test]
    fn hostile_numbers_never_panic() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.profile = vec![[f32::NAN, 0.0], [1.0, f32::INFINITY], [3.0, 3.0]];
        app.sk.stage = Stage::Ready;
        for m in Make::ALL {
            app.sk.make = m;
            app.sk.height = f32::NAN;
            app.sk.loft_scale = f32::INFINITY;
            app.sk.sweep_scale = -1.0;
            assert!(!app.sketch_make(&ctx), "{m:?}");
            assert!(!app.sk.msg.is_empty());
        }
        app.sk.profile = sketch::rect([0.0, 0.0], [10.0, 10.0]);
        app.sk.height = f32::NAN;
        app.sk.make = Make::Extrude;
        assert!(app.sketch_make(&ctx), "a nonsense height is replaced");
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        assert!(app.scene.shape(g, s).expect("there").size[0].is_finite());
        // clicks with nonsense coordinates
        app.sk.reset();
        app.sk.pen = Pen::Polygon;
        for p in [[f32::NAN, 0.0], [1e30, 1e30], [f32::NEG_INFINITY, 2.0]] {
            app.sk.click(p, false);
        }
        assert!(app.sk.enter().is_none());
        // the scene is full: nothing is made
        while app.scene.shape_count() < MAX_SHAPES {
            app.add_shape(&ctx, Kind::Sphere);
        }
        app.sk.reset();
        app.sk.profile = sketch::rect([0.0, 0.0], [10.0, 10.0]);
        app.sk.stage = Stage::Ready;
        assert!(!app.sketch_make(&ctx));
    }

    /// `YT_OUT=/tmp/sk.yura cargo test -p meltcad dump_sketch_scene -- --ignored` writes a scene made
    /// only with the sketch calls (one of every kind), to look at with `meltcad --render`.
    #[test]
    #[ignore]
    fn dump_sketch_scene() {
        let Ok(out) = std::env::var("YT_OUT") else { return };
        let ctx = egui::Context::default();
        let mut app = app();
        app.scene.groups[0].shapes.clear();
        app.add_group();
        app.sel = Some(Pick::Group(0));
        app.set_sketch_tool(true);
        let draw = |app: &mut App, plane: PlaneSel, pen: Pen, smooth: bool, pts: &[[f32; 2]]| {
            app.sketch_set_plane(plane);
            app.sk.pen = pen;
            app.sk.smooth = smooth;
            for p in pts {
                app.sk.click(*p, false);
            }
            let done = if matches!(pen, Pen::Rect | Pen::Circle) { None } else { app.sk.enter() };
            if let Some(d) = done {
                app.sketch_done(d);
            }
        };
        // a floor spline blob pushed up
        draw(&mut app, PlaneSel::Floor, Pen::Spline, true, &[[-60.0, -10.0], [-40.0, -30.0], [-15.0, -20.0], [-20.0, 10.0], [-50.0, 20.0]]);
        app.sk.make = Make::Extrude;
        app.sk.height = 25.0;
        assert!(app.sketch_make(&ctx));
        // a vase on the front plane
        draw(&mut app, PlaneSel::Front, Pen::Spline, true, &[[0.0, 0.0], [18.0, 0.0], [22.0, 20.0], [12.0, 40.0], [14.0, 55.0], [0.0, 55.0]]);
        app.sk.make = Make::Revolve;
        assert!(app.sketch_make(&ctx));
        // a loft from a hexagon-ish polygon to a small circle
        draw(&mut app, PlaneSel::Floor, Pen::Polygon, false, &[[30.0, -20.0], [50.0, -20.0], [60.0, 0.0], [50.0, 20.0], [30.0, 20.0], [20.0, 0.0]]);
        app.sk.make = Make::Loft;
        app.sk.loft_top = LoftTop::Scaled;
        app.sk.loft_scale = 0.4;
        app.sk.height = 40.0;
        assert!(app.sketch_make(&ctx));
        // a swept square along a curve in the air
        draw(&mut app, PlaneSel::Front, Pen::Line, true, &[[-60.0, 70.0], [-30.0, 90.0], [0.0, 70.0], [30.0, 90.0], [60.0, 70.0]]);
        draw(&mut app, PlaneSel::Floor, Pen::Rect, false, &[]);
        app.sk.click([-3.0, -3.0], false);
        let p = app.sk.click([3.0, 3.0], false).expect("rect");
        app.sketch_done(p);
        app.sk.make = Make::Sweep;
        app.sk.sweep_scale = 1.5;
        assert!(app.sketch_make(&ctx));
        // a block on a side plane
        draw(&mut app, PlaneSel::Side, Pen::Polygon, false, &[[-40.0, 0.0], [-10.0, 0.0], [-25.0, 25.0]]);
        app.sk.make = Make::Extrude;
        app.sk.height = 18.0;
        app.sk.flip = true;
        assert!(app.sketch_make(&ctx));
        let _ = std::fs::write(out, app.scene.to_json());
    }
}
