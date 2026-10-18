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
use crate::model::{Kind, MAX_OUTLINE, MAX_PATH, MAX_SHAPES, Material, Op, Pick, Scene, Shape};
use crate::sdf::V3;
use crate::sketch::{self, Frame, SketchError, SnapGeometry, SnapHit, SnapKind, Which};
use crate::sketch_dimensions::Dimensions;
use crate::sketch_edit_ops::{self, EditError};
use crate::style;

/// Pixels within which a click on the first point closes the outline.
const CLOSE_PX: f32 = 11.0;
/// Pixels within which the mouse is on a point of an outline being edited.
const POINT_PX: f32 = 12.0;
/// Pixels within which the mouse is on a line of an outline being edited.
const EDGE_PX: f32 = 9.0;
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
    /// An XY plane at an explicitly entered height.
    Height,
    /// A snapshot of the current camera's image plane.
    Camera,
    /// A saved movable and rotatable user-defined plane.
    Custom,
}

impl PlaneSel {
    pub const ALL: [PlaneSel; 7] =
        [PlaneSel::Floor, PlaneSel::Front, PlaneSel::Side, PlaneSel::OnShape, PlaneSel::Height, PlaneSel::Camera, PlaneSel::Custom];
    pub fn label(self) -> &'static str {
        match self {
            PlaneSel::Floor => "XY (Floor)",
            PlaneSel::Front => "XZ (Front)",
            PlaneSel::Side => "YZ (Side)",
            PlaneSel::OnShape => "On selected shape",
            PlaneSel::Height => "Set height",
            PlaneSel::Camera => "Parallel to camera",
            PlaneSel::Custom => "Custom plane",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            PlaneSel::Floor => "Sketch on the floor (XY). View from above",
            PlaneSel::Front => "Sketch on the front (XZ). Extrudes toward you",
            PlaneSel::Side => "Sketch on the side (YZ). Extrudes toward the right",
            PlaneSel::OnShape => "Sketch on a horizontal plane at the top of the selected shape",
            PlaneSel::Height => "Sketch on a horizontal XY plane at a set Z height",
            PlaneSel::Camera => "Fixes a plane parallel to the current camera view through the camera target. Click again to re-take it",
            PlaneSel::Custom => "Custom plane set by position XYZ and rotation XYZ. Settings are kept when returning from other planes",
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
            Pen::Polygon => "Polygon",
            Pen::Rect => "Rectangle",
            Pen::Circle => "Circle",
            Pen::Spline => "Spline",
            Pen::Line => "Line",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            Pen::Polygon => "Click to place corners, then close the polygon",
            Pen::Rect => "Drag (or click twice) to draw a rectangle",
            Pen::Circle => "Drag from the center outward to draw a circle",
            Pen::Spline => "Draw a smooth closed curve through the clicked points",
            Pen::Line => "Draw an open line to use as a sweep path",
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
            Make::Extrude => "Extrude",
            Make::Revolve => "Revolve",
            Make::Loft => "Loft",
            Make::Sweep => "Sweep",
        }
    }
    pub(crate) fn kind(self) -> Kind {
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
    Hole,
    /// The solid exists already (a live shape in the scene): drag its height handle, then accept.
    Shape,
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
    pub edge_sel: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct DragPoint {
    pub idx: usize,
    /// The outline before the drag (put back when the result would cross itself).
    pub before: Vec<[f32; 2]>,
    /// Lightweight preview; the actual 3D shape is updated only on release.
    pub candidate: Vec<[f32; 2]>,
}

/// What the mouse is on while editing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hover {
    Point(usize),
    /// A line after corner `.0`; a click adds a point at `.1` (outline coordinates).
    Edge(usize, [f32; 2]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOp {
    Trim,
    Offset,
    Fillet,
}

impl EditOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Trim => "Trim",
            Self::Offset => "Offset",
            Self::Fillet => "Fillet",
        }
    }
}

pub struct State {
    /// The sketch tool is on.
    pub on: bool,
    pub plane: PlaneSel,
    /// The plane being drawn on (2D draft coordinates are relative to this frame).
    pub frame: Frame,
    /// Height of the user-selected XY plane.
    pub level_z: f32,
    /// Captured at the moment the camera plane is selected, never updated on camera orbit.
    pub camera_frame: Frame,
    /// Saved independently of the preset planes so it can be reselected.
    pub custom_frame: Frame,
    /// Editing angles in degrees (X, Y, Z), avoiding repeated Euler decomposition on every frame.
    pub custom_angles: V3,
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
    /// Locked snap and the visual marker shown next to the cursor.
    pub snap_hit: Option<SnapHit>,
    /// Optional angular constraint, without changing existing modifier shortcuts.
    pub angle_snap: bool,
    /// The finished profile and the plane it was drawn on.
    pub profile: Vec<[f32; 2]>,
    pub pframe: Frame,
    /// The loft's drawn top profile.
    pub profile2: Vec<[f32; 2]>,
    pub holes: Vec<Vec<[f32; 2]>>,

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
    pub edit_op: Option<EditOp>,
    pub edit_index: Option<usize>,
    pub edit_value: f32,
    /// Transient precise numeric editor (saved geometry remains in the existing profile).
    pub dimension: Option<Dimensions>,
    /// The live solid made from the closed outline (group, shape). It is a real shape in the
    /// scene; undo steps are made only when it is accepted.
    pub live: Option<(usize, usize)>,
    /// The live solid's group was made for it (removed again when the solid is discarded).
    pub live_group: bool,
    /// The selection before the live solid was made.
    pub live_sel: Option<Pick>,
    /// The height handle is being dragged.
    pub handle_drag: bool,
    /// The plane was chosen by hand (it is not picked automatically any more).
    pub plane_manual: bool,
    /// The viewport waits for a plane to be picked: the first click on a face or the ground
    /// chooses the plane and places the first point.
    pub picking: bool,
    /// The plane under the mouse while picking (and whether it is a face of a shape).
    pub hover_frame: Option<(Frame, bool, V3)>,
    /// The chosen plane came from a face: its grid is a square around the picked point.
    pub plane_square: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            on: false,
            plane: PlaneSel::Floor,
            frame: Frame::floor(),
            level_z: 0.0,
            camera_frame: Frame::floor(),
            custom_frame: Frame::floor(),
            custom_angles: [0.0; 3],
            pen: Pen::Polygon,
            smooth: true,
            stage: Stage::Draw,
            pts: Vec::new(),
            anchor: None,
            dragging: false,
            cursor: None,
            mouse: None,
            snap_hit: None,
            angle_snap: false,
            profile: Vec::new(),
            pframe: Frame::floor(),
            profile2: Vec::new(),
            holes: Vec::new(),
            path: Vec::new(),
            make: Make::Extrude,
            height: 20.0,
            flip: false,
            loft_top: LoftTop::Scaled,
            loft_scale: 0.5,
            sweep_scale: 1.0,
            msg: String::new(),
            edit: None,
            edit_op: None,
            edit_index: None,
            edit_value: 2.0,
            dimension: None,
            live: None,
            live_group: false,
            live_sel: None,
            handle_drag: false,
            plane_manual: false,
            picking: false,
            hover_frame: None,
            plane_square: false,
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
        self.dimension = None;
        self.snap_hit = None;
    }

    /// Back to a blank page (the settings stay).
    pub fn reset(&mut self) {
        self.clear_drawing();
        self.stage = Stage::Draw;
        self.profile.clear();
        self.profile2.clear();
        self.holes.clear();
        self.loft_top = LoftTop::Scaled;
        self.msg.clear();
        // (the live solid, if any, stays in the scene: removing it is `App::live_discard`)
        self.live = None;
        self.live_group = false;
        self.handle_drag = false;
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
            self.msg = format!("Up to {limit} points");
            return;
        }
        if self.corners()
            && let Some(last) = self.pts.last().copied()
            && self.pts.len() >= 2
            && sketch::crosses_polyline(&self.pts, last, p, false)
        {
            self.msg = "A line there would cross another. Click a different spot".to_string();
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
            Stage::LoftTop if self.live.is_some() => {
                self.stage = Stage::Shape;
                if self.profile2.is_empty() {
                    self.loft_top = LoftTop::Scaled;
                }
                self.msg.clear();
                false
            }
            Stage::SweepPath if self.live.is_none() && self.profile.is_empty() => true,
            Stage::LoftTop | Stage::SweepPath | Stage::Hole => {
                self.stage = Stage::Ready;
                self.msg.clear();
                false
            }
            // (the live solid is discarded by the App, which owns the scene)
            Stage::Shape => {
                self.stage = Stage::Draw;
                self.msg.clear();
                false
            }
            Stage::Draw => true,
        }
    }
}

// ------------------------------------------------------------------ the App side: turning it on, the plane, the camera

pub(crate) const COLORS: [[f32; 3]; 6] =
    [[0.93, 0.56, 0.45], [0.60, 0.72, 0.98], [0.62, 0.90, 0.74], [0.96, 0.80, 0.45], [0.76, 0.66, 0.96], [0.96, 0.66, 0.82]];

impl App {
    /// Is the sketch tool on, or an outline being edited? (The gizmo and the node editing stay out of the way.)
    pub fn sketch_active(&self) -> bool {
        self.sk.on || self.sk.edit.is_some()
    }

    /// Switch the sketch tool on or off. It excludes the other drawing tools.
    pub fn set_sketch_tool(&mut self, on: bool) {
        self.edit_cancel_drag();
        self.sk.edit = None;
        self.sk.edit_op = None;
        self.sk.edit_index = None;
        // (leaving the tool keeps a live solid that is on the screen)
        self.sk.reset();
        self.sk.on = on;
        if on {
            // the plane follows the selection until one is chosen by hand
            if !self.sk.plane_manual {
                self.sk.plane = if matches!(self.sel, Some(Pick::Shape(..))) { PlaneSel::OnShape } else { PlaneSel::Floor };
            }
            if self.sk.make == Make::Sweep {
                self.sk.make = Make::Extrude;
            }
            self.finish_drawing();
            self.ct.drawing = false;
            self.set_pipe_tool(false);
            self.sk.frame = self.sketch_plane();
            // the viewport waits for a click on a face or the ground (no glide until then)
            self.sk.picking = true;
            self.sk.hover_frame = None;
            self.sk.plane_square = false;
        } else {
            self.sk.plane_manual = false;
            self.sk.picking = false;
            self.sk.hover_frame = None;
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
        if style::wide_button(ui, "Sketch", "Sketch (draw on a plane, then Extrude, Revolve, Loft or Sweep)", on, None, glyph).clicked() {
            self.set_sketch_tool(!on);
        }
    }

    /// Strip buttons for Extrude, Loft and Sweep: start the sketch with that way of making chosen
    /// (or switch to it while sketching). Draw the outline, then press "Make".
    pub fn sketch_make_buttons(&mut self, ui: &mut Ui) {
        type Icon = fn(&egui::Painter, egui::Rect, Color32);
        let items: [(Make, &str, &str, Icon); 3] = [
            (Make::Extrude, "Extrude", "Draw an outline, then push it into a solid", crate::sketch_icons::extrude),
            (Make::Loft, "Loft", "Draw an outline and blend it into a second outline", crate::sketch_icons::loft),
            (Make::Sweep, "Sweep", "Draw an outline, then pull it along a path", crate::sketch_icons::sweep),
        ];
        for (m, name, tip, icon) in items {
            let on = self.sk.on && self.sk.make == m;
            if style::wide_button(ui, name, tip, on, None, icon).clicked() {
                if on {
                    self.set_sketch_tool(false);
                } else {
                    if !self.sk.on {
                        self.set_sketch_tool(true);
                    }
                    self.sketch_choose_make(m);
                }
            }
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
            PlaneSel::Height => Frame::horizontal(self.sk.level_z),
            PlaneSel::Camera => self.sk.camera_frame,
            PlaneSel::Custom => self.sk.custom_frame,
        }
    }

    /// Glide the camera to look straight at a plane, keeping the plane through the view's middle.
    pub fn sketch_fly(&mut self, f: &Frame) {
        let n = f.normal();
        // Camera::look clamps the pole slightly, avoiding its singular right vector.
        // Unlike the old horizontal-only calculation this also faces oblique planes.
        let yaw = if n[0].hypot(n[1]) < 1e-5 { self.cam.yaw } else { n[0].atan2(-n[1]) };
        let pitch = n[2].clamp(-1.0, 1.0).asin();
        let mut goal = self.fly.unwrap_or(self.cam);
        goal.look(yaw, pitch);
        // the target moves onto the plane
        let t = goal.target;
        let off = crate::sdf::dot(crate::sdf::sub(t, f.origin), n);
        goal.target = crate::sdf::sub(t, crate::sdf::mul(n, off));
        self.fly = Some(goal);
    }

    /// Choose a plane (and glide to it). A camera-aligned plane is captured once,
    /// not rebuilt after every orbit. A finished profile keeps its independent pframe.
    pub fn sketch_set_plane(&mut self, plane: PlaneSel) {
        if plane == PlaneSel::Camera {
            let (right, up, _) = self.cam.basis();
            let Some(frame) = Frame::from_axes(self.cam.target, right, up) else {
                self.sk.msg = "Can't make a plane from the camera view. Adjust the view".to_string();
                return;
            };
            self.sk.camera_frame = frame;
        }
        let interrupted = !self.sk.pts.is_empty() || self.sk.anchor.is_some();
        self.sk.plane = plane;
        self.sk.plane_manual = true;
        self.sk.picking = false;
        self.sk.hover_frame = None;
        self.sk.plane_square = false;
        self.sk.clear_drawing();
        self.sk.frame = self.sketch_plane();
        self.sk.msg = if interrupted {
            "Changing plane cleared unfinished points. Finished outlines and solids are unchanged".to_string()
        } else {
            String::new()
        };
        if plane == PlaneSel::Camera {
            // A stale camera fly-to would immediately undo the captured orientation.
            self.fly = None;
        } else {
            let f = self.sk.frame;
            self.sketch_fly(&f);
        }
    }

    /// Move an explicit horizontal plane without changing draft-local point coordinates.
    pub fn sketch_set_level(&mut self, z: f32) -> bool {
        if !z.is_finite() || !(-5000.0..=5000.0).contains(&z) {
            return false;
        }
        self.sk.level_z = z;
        if self.sk.plane == PlaneSel::Height {
            self.sk.frame = Frame::horizontal(z);
        }
        true
    }

    /// Edit a free plane. Existing 2D draft points stay in the same local coordinates:
    /// moving/turning the plane moves the entire uncommitted sketch with it.
    pub fn sketch_edit_custom_plane(&mut self, origin: V3, angles: V3) -> bool {
        let Some(frame) = Frame::oriented(origin, angles) else {
            self.sk.msg = "Plane position or angle is invalid".to_string();
            return false;
        };
        self.sk.custom_frame = frame;
        self.sk.custom_angles = angles;
        if self.sk.plane == PlaneSel::Custom {
            self.sk.frame = frame;
        }
        self.sk.msg.clear();
        true
    }

    /// Convert the currently chosen workplane into an editable, reselectable copy.
    pub fn sketch_copy_plane_to_custom(&mut self) {
        let frame = self.sk.frame;
        self.sk.custom_frame = frame;
        self.sk.custom_angles = frame.euler();
        self.sketch_set_plane(PlaneSel::Custom);
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

    /// Geometry near the current plane, including committed profile and saved sweep path.
    /// A point being dragged is intentionally omitted (otherwise it sticks to its old position).
    fn sketch_snap_geometry(&self, frame: &Frame, omit: Option<usize>) -> SnapGeometry {
        let mut geo = SnapGeometry::default();
        if let Some(e) = &self.sk.edit {
            if let Some(shape) = self.scene.shape(e.g, e.s) {
                geo.add_outline(sketch::outline_of(shape, e.which), true, omit);
            }
        } else {
            let pen = self.sk.active_pen();
            geo.add_outline(&self.sk.pts, false, None);
            if let Some(a) = self.sk.anchor {
                geo.points.push(SnapHit { at: a, kind: if pen == Pen::Circle { SnapKind::Center } else { SnapKind::Endpoint } });
            }
            if self.sk.stage == Stage::LoftTop {
                geo.add_outline(&self.sk.profile, true, None);
            }
            if self.sk.path.len() >= 2 {
                let points: Vec<[f32; 2]> = self
                    .sk
                    .path
                    .iter()
                    .filter(|p| {
                        let d = crate::sdf::dot(crate::sdf::sub(**p, frame.origin), frame.normal());
                        d.abs() <= 0.01
                    })
                    .map(|p| frame.from_world(*p))
                    .collect();
                geo.add_outline(&points, false, None);
            }
        }
        geo
    }

    /// Both the visible cursor and the actual drawing/edit point use this resolver.
    fn sketch_resolve_snap(
        &self,
        rect: Rect,
        mouse: Pos2,
        step: f32,
        frame: &Frame,
        old: Option<SnapHit>,
        omit: Option<usize>,
    ) -> Option<([f32; 2], Option<SnapHit>)> {
        let (o, d) = self.cam.ray_at(rect, mouse);
        let raw = frame.hit(o, d)?;
        let mut geo = self.sketch_snap_geometry(frame, omit);
        let mut hits: Vec<(SnapHit, [f32; 2])> = Vec::new();
        let mut add = |hit: SnapHit| {
            if let Some(px) = self.sketch_px(rect, frame, hit.at)
                && (px - mouse).length() <= sketch::SNAP_HOLD_PX
                && hits.len() < 1536
            {
                hits.push((hit, [px.x, px.y]));
            }
        };
        add(SnapHit { at: [0.0, 0.0], kind: SnapKind::Origin });
        for hit in geo.points.drain(..) {
            add(hit);
        }
        // Only consider edges close enough on screen; this bounds intersection work.
        let nearby: Vec<_> = geo
            .edges
            .iter()
            .copied()
            .filter(|(a, b)| {
                let (Some(a), Some(b)) = (self.sketch_px(rect, frame, *a), self.sketch_px(rect, frame, *b)) else { return false };
                let ab = b - a;
                let t = ((mouse - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                (mouse - (a + ab * t)).length() <= sketch::SNAP_HOLD_PX
            })
            .take(64)
            .collect();
        for i in 0..nearby.len() {
            for j in i + 1..nearby.len() {
                let (a, b) = nearby[i];
                let (c, d) = nearby[j];
                if let Some(at) = sketch::snap_intersection(a, b, c, d) {
                    add(SnapHit { at, kind: SnapKind::Intersection });
                }
            }
        }
        let grid = SnapHit { at: sketch::snap(raw, step), kind: SnapKind::Grid };
        add(grid);
        let anchor = self.sk.pts.last().copied().or(self.sk.anchor);
        if let Some(a) = anchor {
            add(SnapHit { at: [raw[0], a[1]], kind: SnapKind::Horizontal });
            add(SnapHit { at: [a[0], raw[1]], kind: SnapKind::Vertical });
            if self.sk.angle_snap
                && let Some(at) = sketch::snap_angle(a, raw, 15.0)
            {
                add(SnapHit { at, kind: SnapKind::Angle });
            }
        }
        // Keep original grid semantics when it is more than 11px away from the
        // raw cursor; the explicit geometric snaps themselves are pixel limited.
        let chosen = sketch::choose_screen_snap([mouse.x, mouse.y], &hits, old);
        Some((chosen.map_or(grid.at, |hit| hit.at), chosen))
    }

    // ----- what a finished gesture does -----

    /// A profile or a line has been drawn.
    pub fn sketch_done(&mut self, done: Finished) {
        match done {
            Finished::Profile(p) => {
                if self.sk.stage == Stage::Hole {
                    let mut candidate = self.sk.holes.clone();
                    candidate.push(p);
                    match sketch::validate_holes(&self.sk.profile, &candidate) {
                        Ok(h) => {
                            self.sk.holes = h;
                            self.sk.msg.clear();
                        }
                        Err(e) => self.sk.msg = e.message(),
                    }
                    self.sk.stage = Stage::Ready;
                    return;
                }
                if self.sk.stage == Stage::LoftTop {
                    self.sk.profile2 = p;
                    self.sk.loft_top = LoftTop::Drawn;
                    self.sk.make = Make::Loft;
                    if self.sk.live.is_some() {
                        self.sk.stage = Stage::Shape;
                        self.live_update();
                        return;
                    }
                } else {
                    self.sk.profile = p;
                    self.sk.pframe = self.sk.frame;
                    self.sk.profile2.clear();
                    self.sk.holes.clear();
                    self.sk.loft_top = LoftTop::Scaled;
                    if self.sk.make == Make::Revolve && !self.sk.pframe.is_vertical() {
                        self.sk.make = Make::Extrude;
                    }
                    // Extrude and Loft: the solid appears right away
                    if matches!(self.sk.make, Make::Extrude | Make::Loft) && self.live_begin() {
                        return;
                    }
                }
                self.sk.stage = Stage::Ready;
            }
            Finished::Line(l) => {
                let f = self.sk.frame;
                self.sk.path = l.iter().map(|q| f.to_world(*q)).collect();
                if self.sk.stage == Stage::SweepPath && self.sk.profile.is_empty() {
                    // path first: a round tube follows it
                    self.sweep_path_done();
                } else if self.sk.stage == Stage::SweepPath {
                    self.sk.stage = Stage::Ready;
                    self.sk.make = Make::Sweep;
                    self.sk.frame = self.sk.pframe;
                } else {
                    self.sk.msg = format!("Path saved with {} points. Draw an outline and use Sweep", self.sk.path.len());
                }
            }
        }
    }

    /// Make the shape the card is set to. True when it was made.
    pub fn sketch_make(&mut self, ctx: &egui::Context) -> bool {
        if self.sk.live.is_some() {
            let ok = self.live_alive();
            self.live_accept(ctx);
            return ok;
        }
        if self.scene.shape_count().saturating_add(1 + self.sk.holes.len()) > MAX_SHAPES {
            self.sk.msg = format!("Up to {MAX_SHAPES} shapes");
            return false;
        }
        if !self.sk.holes.is_empty() && self.sk.make != Make::Extrude {
            self.sk.msg = "Sketches with holes can only be extruded".to_string();
            return false;
        }
        if let Err(e) = sketch::validate_holes(&self.sk.profile, &self.sk.holes) {
            self.sk.msg = e.message();
            return false;
        }
        if !self.sk.holes.is_empty() && self.scene.groups.len() >= crate::model::MAX_GROUPS {
            self.sk.msg = "Group limit reached".to_string();
            return false;
        }
        let built = match self.build_shape() {
            Ok(b) => b,
            Err(e) => {
                self.sk.msg = e.message();
                return false;
            }
        };
        let make = self.sk.make;
        let Some(g) = (if self.sk.holes.is_empty() {
            self.target_group()
        } else {
            let mut group = crate::model::Group::new("Extrude with holes");
            group.sketch_holes = true;
            self.scene.groups.push(group);
            Some(self.scene.groups.len() - 1)
        }) else {
            return false;
        };
        let mut color = COLORS[self.scene.shape_count() % COLORS.len()];
        if self.new_op == Op::Subtract {
            color = [0.9, 0.35, 0.4];
        }
        let mut sh = Shape::new(make.kind(), built.pos, Material { color, ..Material::default() });
        sketch::apply(&built, &mut sh);
        sh.name = make.label().to_string();
        sh.op = if self.sk.holes.is_empty() { self.new_op } else { Op::Add };
        sh.blend = if self.sk.holes.is_empty() { 2.0 } else { 0.0 };
        let mut cutouts = Vec::new();
        for (i, contour) in self.sk.holes.iter().enumerate() {
            let Ok(hole) = sketch::extrude(&self.sk.pframe, contour, self.sk.height, self.sk.flip) else {
                self.sk.msg = "Failed to extrude holes".to_string();
                return false;
            };
            let mut cutter = Shape::new(Kind::Extrude, hole.pos, sh.material);
            sketch::apply(&hole, &mut cutter);
            cutter.name = format!("Hole {}", i + 1);
            cutter.op = Op::Subtract;
            cutter.blend = 0.0;
            cutouts.push(cutter);
        }
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        group.shapes.push(sh);
        group.shapes.extend(cutouts);
        let idx = group.shapes.len() - 1;
        self.sel = Some(Pick::Shape(g, idx));
        self.sk.reset();
        self.sk.frame = self.sketch_plane();
        self.say(ctx, format!("Created {} (use Edit Sketch to change the outline)", make.label()));
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
            Stage::Ready | Stage::LoftTop | Stage::Hole | Stage::Shape => self.sk.frame = self.sk.pframe,
            _ if self.sk.pts.is_empty() && self.sk.anchor.is_none() => self.sk.frame = self.sketch_plane(),
            _ => {}
        }
        if self.picking_now() {
            // the first click picks the plane and (below) goes on to place the first point
            self.plane_pick_input(ui, resp, rect, pos, mods);
            if self.picking_now() {
                out.over = self.sk.hover_frame.is_some();
                out.taken = pos.is_some() && (resp.clicked_by(egui::PointerButton::Primary) || resp.secondary_clicked());
                return out;
            }
        }
        let frame = self.sk.frame;
        let step = step_for(mods);
        let mouse = pos.filter(|p| resp.hovered() && !over_gnomon(*p));
        // Losing the pointer during a shape gesture cancels it, rather than
        // reusing its first corner on the next click.
        if self.sk.dragging && ui.input(|i| i.pointer.interact_pos()).is_none() {
            self.sk.cancel_drag();
        }
        self.sk.mouse = mouse;
        let resolved = mouse.and_then(|p| self.sketch_resolve_snap(rect, p, step, &frame, self.sk.snap_hit, None));
        self.sk.cursor = resolved.map(|r| r.0);
        self.sk.snap_hit = resolved.and_then(|r| r.1);
        out.over = self.sk.cursor.is_some();
        if self.sk.stage == Stage::Shape {
            let shaped = self.live_input(ui, resp, rect, pos, mods);
            return Input { over: out.over || shaped.over, taken: shaped.taken, busy: shaped.busy, ..out };
        }
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
        let mut measured = None;
        if matches!(self.sk.active_pen(), Pen::Rect | Pen::Circle) {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(o) = ui.input(|i| i.pointer.press_origin()).or(pos).filter(|o| !over_gnomon(*o))
                && let Some((q, _)) = self.sketch_resolve_snap(rect, o, step, &frame, None, None)
            {
                self.sk.press(q);
            }
            if self.sk.dragging {
                out.taken = true;
                out.busy = true;
                if !primary_down || resp.drag_stopped() {
                    match self.sk.cursor {
                        Some(q) => {
                            let a = self.sk.anchor;
                            done = self.sk.release(q);
                            if done.is_some() {
                                measured = a.map(|p| (p, q));
                            }
                        }
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
            let a = self.sk.anchor;
            done = done.or_else(|| self.sk.click(q, close));
            if done.is_some() && matches!(self.sk.active_pen(), Pen::Rect | Pen::Circle) {
                measured = a.map(|p| (p, q));
            } else if done.is_none() {
                // A new click supersedes any previous point/segment dimension editor.
                self.sk.dimension = None;
            }
            out.taken = true;
        }
        // left click places, right click confirms (the same in every drawing tool)
        if resp.secondary_clicked() && mouse.is_some() {
            done = done.or_else(|| self.sk.enter());
            out.taken = true;
        }
        if let Some(d) = done {
            let is_profile = matches!(d, Finished::Profile(_));
            let top = self.sk.stage == Stage::LoftTop;
            let pen = self.sk.active_pen();
            self.sketch_done(d);
            if is_profile && let Some((a, b)) = measured {
                let p = if top { &self.sk.profile2 } else { &self.sk.profile };
                self.sk.dimension = Dimensions::for_shape(pen, a, b, p, top);
            }
        }
        // Every primary sketch gesture belongs to the sketch, including drags
        // which have not produced a point. Never fall through to object picking.
        if !mods.alt
            && (resp.drag_started_by(egui::PointerButton::Primary)
                || resp.dragged_by(egui::PointerButton::Primary)
                || resp.drag_stopped_by(egui::PointerButton::Primary)
                || resp.clicked_by(egui::PointerButton::Primary)
                || resp.secondary_clicked())
        {
            out.taken = true;
        }
        out
    }

    // ----- the keys -----

    /// Enter, Backspace / Delete and Esc while the tool is on. Call before the other shortcuts.
    /// Delete passes through to shape deletion only when drawing has no pending point.
    pub fn sketch_keys(&mut self, ctx: &egui::Context) -> bool {
        if !self.sketch_active() {
            return false;
        }
        use egui::{Key, Modifiers};
        let hit = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if self.sk.dimension.as_ref().is_some_and(|d| d.editing) {
            if hit(Key::Tab) {
                if let Some(d) = self.sk.dimension.as_mut() {
                    d.selected = (d.selected + 1) % 2;
                    d.needs_focus = true;
                }
            }
            if hit(Key::Escape) {
                if let Some(d) = self.sk.dimension.as_mut() {
                    let profile = if d.top_profile { &mut self.sk.profile2 } else { &mut self.sk.profile };
                    d.cancel(&mut self.sk.pts, profile);
                }
            } else if hit(Key::Enter) {
                if let Some(d) = self.sk.dimension.as_mut() {
                    let profile = if d.top_profile { &self.sk.profile2 } else { &self.sk.profile };
                    d.accept(&self.sk.pts, profile);
                }
            }
            // Stop all global shortcuts, including the numeric view keys.
            return true;
        }
        if self.sk.edit.is_none() && (self.sk.dimension.is_some() || !self.sk.pts.is_empty()) && hit(Key::Tab) {
            if let Some(d) = self.sk.dimension.as_mut() {
                d.begin_again();
            } else if let Some(d) = Dimensions::for_last_point(&self.sk.pts, self.sk.active_pen()) {
                self.sk.dimension = Some(d);
            }
            return true;
        }
        // A focused numeric field owns its Enter/Delete/Esc and text input.
        if ctx.egui_wants_keyboard_input() {
            return false;
        }
        let sketch_uses_delete = self.sk.edit.is_some() || !self.sk.pts.is_empty() || self.sk.anchor.is_some();
        let del = (sketch_uses_delete && hit(Key::Delete)) | hit(Key::Backspace);
        let enter = hit(Key::Enter);
        let esc = hit(Key::Escape);
        if self.sk.edit.is_some() {
            if self.sk.edit_op.is_some() {
                if esc {
                    self.sk.edit_op = None;
                    self.sk.edit_index = None;
                } else if enter {
                    self.edit_commit_operation();
                }
                return esc || enter;
            }
            if del {
                self.sketch_edit_delete();
            }
            if esc {
                if !self.edit_cancel_drag() {
                    self.sk.edit = None;
                }
            } else if enter {
                self.edit_release();
                self.sk.edit = None;
            }
            return del || enter || esc;
        }
        if self.sk.stage == Stage::Shape && self.sk.live.is_some() {
            // Enter keeps the solid, Esc / Delete throw it away
            let delete = hit(Key::Delete);
            if enter {
                self.live_accept(ctx);
            } else if esc || delete {
                self.live_discard();
            }
            return del || enter || esc || delete;
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
        del || enter || esc
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
        self.sk.edit = Some(Edit { g, s, which: Which::Main, sel: None, drag: None, hover: None, edge_sel: None });
        self.sk.edit_op = None;
        self.sk.edit_index = None;
        if let Some(f) = self.scene.shape(g, s).and_then(|sh| sketch::edit_frame(sh, Which::Main)) {
            self.sketch_fly(&f);
        }
    }

    /// The button in the shape panel: edit this shape's outline (or stop).
    pub fn sketch_edit_button(&mut self, ui: &mut Ui, g: usize, s: usize) {
        let editing = self.sk.edit.as_ref().is_some_and(|e| e.g == g && e.s == s);
        let tip = "Edit this shape's outline by moving its points in the viewport";
        if style::pill(ui, if editing { "Finish Editing" } else { "Edit Sketch" }, editing).on_hover_text(tip).clicked() {
            if editing {
                self.edit_release();
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

    /// Compute a non-destructive preview; validation occurs again on commit.
    fn edit_preview(&self) -> Option<Result<Vec<[f32; 2]>, EditError>> {
        let e = self.sk.edit.as_ref()?;
        let op = self.sk.edit_op?;
        let shape = self.scene.shape(e.g, e.s)?;
        let outline = sketch::outline_of(shape, e.which);
        Some(match op {
            EditOp::Trim => self.sk.edit_index.map_or(Err(EditError::CannotTrim), |i| sketch_edit_ops::trim_edge(outline, i)),
            EditOp::Offset => sketch_edit_ops::offset_profile(outline, self.sk.edit_value),
            EditOp::Fillet => {
                self.sk.edit_index.map_or(Err(EditError::InvalidOutline), |i| sketch_edit_ops::fillet_corner(outline, i, self.sk.edit_value))
            }
        })
    }

    fn edit_commit_operation(&mut self) {
        let Some(op) = self.sk.edit_op else { return };
        let Some(result) = self.edit_preview() else { return };
        match result {
            Err(err) => self.sk.msg = err.message().to_owned(),
            Ok(points) => {
                if op == EditOp::Offset {
                    // Keep the source solid untouched. Offer the new contour as a pending sketch.
                    let Some(e) = self.sk.edit.as_ref() else { return };
                    let Some(sh) = self.scene.shape(e.g, e.s) else { return };
                    let Some(frame) = sketch::edit_frame(sh, e.which) else { return };
                    self.sk.edit = None;
                    self.sk.on = true;
                    self.sk.reset();
                    self.sk.profile = points;
                    self.sk.pframe = frame;
                    self.sk.frame = frame;
                    self.sk.stage = Stage::Ready;
                    self.sk.msg = "Offset outline created. Make it solid or press Esc to discard".to_owned();
                } else if let Some((outline, _)) = self.edit_outline_mut() {
                    *outline = points;
                    self.sk.msg.clear();
                }
                self.sk.edit_index = None;
                self.sk.edit_op = None;
            }
        }
    }

    /// The button went down on point `idx`: start moving it.
    pub fn edit_begin_drag(&mut self, idx: usize) -> bool {
        let Some((ol, _)) = self.edit_outline_mut() else { return false };
        if idx >= ol.len() {
            return false;
        }
        let before = ol.clone();
        self.sk.snap_hit = None;
        if let Some(e) = self.sk.edit.as_mut() {
            e.drag = Some(DragPoint { idx, candidate: before.clone(), before });
            e.sel = Some(idx);
            e.edge_sel = None;
        }
        true
    }

    /// Update just the 2D draft while dragging. Do not invalidate the SDF cache every frame.
    pub fn edit_drag(&mut self, rect: Rect, m: Pos2, step: f32) {
        // Dragging outside the viewport should retain the active gesture, not jump to
        // an arbitrarily distant plane intersection.
        let m = pos2(m.x.clamp(rect.left(), rect.right()), m.y.clamp(rect.top(), rect.bottom()));
        let Some((f, _)) = self.edit_target() else { return };
        let Some(idx) = self.sk.edit.as_ref().and_then(|e| e.drag.as_ref()).map(|d| d.idx) else { return };
        let Some((q, hit)) = self.sketch_resolve_snap(rect, m, step, &f, self.sk.snap_hit, Some(idx)) else { return };
        self.sk.snap_hit = hit;
        self.sk.mouse = Some(m);
        let revolved = self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s)).is_some_and(|s| s.kind == Kind::Revolve);
        if let Some(d) = self.sk.edit.as_mut().and_then(|e| e.drag.as_mut())
            && let Some(p) = d.candidate.get_mut(d.idx)
        {
            *p = if revolved { [q[0].max(0.0), q[1]] } else { q };
        }
    }

    /// Esc cancels the in-progress gesture but preserves the editing session and selection.
    pub fn edit_cancel_drag(&mut self) -> bool {
        let Some(e) = self.sk.edit.as_mut() else { return false };
        if e.drag.take().is_none() {
            return false;
        }
        e.hover = None;
        self.sk.snap_hit = None;
        self.sk.msg.clear();
        true
    }

    /// On release, validate the draft and atomically replace the solid's outline. A drag is a
    /// single transaction: it is also rejected if any through-hole contour of the same group
    /// would leave the outer profile or touch another hole.
    pub fn edit_release(&mut self) {
        let Some((g, s, which, d)) = self.sk.edit.as_mut().and_then(|e| e.drag.take().map(|d| (e.g, e.s, e.which, d))) else { return };
        self.sk.snap_hit = None;
        let revolved = self.scene.shape(g, s).is_some_and(|sh| sh.kind == Kind::Revolve);
        // the precise-edit rules first (no vertex lost, winding kept), then the solid's own checks
        if let Err(err) = sketch::validate_edit(&d.before, &d.candidate, revolved) {
            self.sk.msg = err.message();
            return;
        }
        let before = d.before;
        let top = which == Which::Top;
        let result = self.scene.shape_mut(g, s).map(|sh| sh.set_sketch_outline(top, d.candidate));
        match result {
            Some(Ok(_)) => {
                if let Err(err) = self.check_edited_hole_group() {
                    // a hole would be crossed: put the old outline back
                    if let Some(sh) = self.scene.shape_mut(g, s) {
                        if top { sh.outline2 = before } else { sh.outline = before }
                    }
                    self.sk.msg = err.message();
                } else {
                    self.sk.msg.clear();
                }
            }
            Some(Err(err)) => self.sk.msg = err,
            None => self.sk.msg = "Shape to edit not found".to_string(),
        }
    }

    fn check_edited_hole_group(&self) -> Result<(), SketchError> {
        let Some(e) = self.sk.edit.as_ref() else { return Ok(()) };
        let Some(group) = self.scene.groups.get(e.g) else { return Ok(()) };
        if !group.sketch_holes {
            return Ok(());
        };
        let Some(outer) = group.shapes.first() else { return Ok(()) };
        let Some(of) = sketch::edit_frame(outer, Which::Main) else { return Err(SketchError::HoleMisaligned) };
        let mut holes = Vec::new();
        for sh in &group.shapes {
            if sh.kind != Kind::Extrude || sh.size[0] != outer.size[0] || sh.rot != outer.rot || sh.scale != outer.scale {
                return Err(SketchError::HoleMisaligned);
            }
        }
        for sh in group.shapes.iter().skip(1) {
            if sh.op != Op::Subtract {
                return Err(SketchError::HoleMisaligned);
            }
            let Some(hf) = sketch::edit_frame(sh, Which::Main) else { return Err(SketchError::HoleMisaligned) };
            if crate::sdf::dot(of.normal(), hf.normal()) < 0.9999 || crate::sdf::dot(crate::sdf::sub(hf.origin, of.origin), of.normal()).abs() > 0.01
            {
                return Err(SketchError::HoleMisaligned);
            }
            holes.push(sh.outline.iter().map(|p| of.from_world(hf.to_world(*p))).collect());
        }
        sketch::validate_holes(&outer.outline, &holes).map(|_| ())
    }

    /// A click without a drag: choose a point, add one on a line, or choose nothing.
    pub fn edit_click(&mut self, rect: Rect, m: Pos2) {
        match self.edit_hit(rect, m) {
            Some(Hover::Point(i)) => {
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = Some(i);
                    e.edge_sel = None;
                }
            }
            Some(Hover::Edge(i, at)) => {
                let added = self
                    .sk
                    .edit
                    .as_ref()
                    .and_then(|e| self.scene.shape(e.g, e.s).map(|sh| (e.g, e.s, e.which, sh.kind, sketch::outline_of(sh, e.which).clone())))
                    .is_some_and(|(g, s, which, kind, mut ol)| {
                        let at = if kind == Kind::Revolve { [at[0].max(0.0), at[1]] } else { at };
                        sketch::insert_point(&mut ol, i, at)
                            && self.scene.shape_mut(g, s).is_some_and(|sh| sh.set_sketch_outline(which == Which::Top, ol).is_ok())
                    });
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = added.then_some(i + 1);
                    e.edge_sel = Some(i);
                }
                if !added {
                    self.sk.msg = format!("Up to {MAX_OUTLINE} points");
                }
            }
            None => {
                if let Some(e) = self.sk.edit.as_mut() {
                    e.sel = None;
                    e.edge_sel = None;
                }
            }
        }
    }

    /// Commit a precision edit atomically; rejected edits never touch the scene.
    fn precision_edit(&mut self, f: impl FnOnce(&[[f32; 2]], bool) -> Result<Vec<[f32; 2]>, SketchError>) -> bool {
        let Some((outline, rev)) = self.edit_outline_mut() else { return false };
        let result = f(outline, rev);
        match result {
            Ok(next) => {
                *outline = next;
                self.sk.msg.clear();
                true
            }
            Err(e) => {
                self.sk.msg = e.message();
                false
            }
        }
    }

    /// A deliberately small alternative to a general constraint solver.
    pub fn precision_vertex(&mut self, idx: usize, point: [f32; 2]) -> bool {
        self.precision_edit(|pts, rev| sketch::move_vertex(pts, idx, point, rev))
    }

    pub fn precision_edge_length(&mut self, idx: usize, length: f32) -> bool {
        self.precision_edit(|pts, rev| sketch::set_edge_length(pts, idx, length, rev))
    }

    pub fn precision_rectangle(&mut self, width: f32, height: f32) -> bool {
        self.precision_edit(|pts, rev| sketch::set_rectangle_size(pts, width, height, rev))
    }

    pub fn precision_circle(&mut self, diameter: f32) -> bool {
        self.precision_edit(|pts, rev| sketch::set_circle_diameter(pts, diameter, rev))
    }

    /// Delete: remove the chosen point (three stay at least).
    pub fn sketch_edit_delete(&mut self) {
        let Some(i) = self.sk.edit.as_ref().and_then(|e| e.sel) else { return };
        let res: Option<Result<(), SketchError>> =
            self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s).map(|s| (e.g, e.s, e.which, sketch::outline_of(s, e.which).clone()))).map(
                |(g, s, which, mut candidate)| {
                    sketch::remove_point(&mut candidate, i)?;
                    self.scene
                        .shape_mut(g, s)
                        .ok_or(SketchError::BadNumber)?
                        .set_sketch_outline(which == Which::Top, candidate)
                        .map_err(|_| SketchError::SelfIntersect)?;
                    Ok(())
                },
            );
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
        if let Some(op) = self.sk.edit_op {
            let mouse = pos.filter(|p| resp.hovered() && !mods.alt);
            let hit = mouse.and_then(|m| self.edit_hit(rect, m));
            if let Some(e) = self.sk.edit.as_mut() {
                e.hover = hit;
            }
            if let Some(m) = mouse {
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.sk.edit_index = match (op, self.edit_hit(rect, m)) {
                        (EditOp::Trim, Some(Hover::Edge(i, _))) => Some(i),
                        (EditOp::Fillet, Some(Hover::Point(i))) => Some(i),
                        _ => None,
                    };
                    out.taken = true;
                }
            }
            if resp.secondary_clicked() && mouse.is_some() {
                self.edit_commit_operation();
                out.taken = true;
            }
            out.over = hit.is_some();
            return out;
        }
        // left click edits, right click confirms and leaves the editor
        if resp.secondary_clicked() && resp.hovered() && self.sk.edit.as_ref().is_some_and(|e| e.drag.is_none()) {
            self.sk.edit = None;
            out.taken = true;
            return out;
        }
        let step = step_for(mods);
        let primary_down = ui.input(|i| i.pointer.primary_down());
        if self.sk.edit.as_ref().is_some_and(|e| e.drag.is_some()) {
            out.over = true;
            out.taken = true;
            out.busy = true;
            match (primary_down, ui.input(|i| i.pointer.interact_pos())) {
                (true, Some(p)) if rect.contains(p) => self.edit_drag(rect, p, step),
                (true, _) => {
                    self.edit_cancel_drag();
                }
                _ => self.edit_release(),
            }
            return out;
        }
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < GNOMON_PX;
        let mouse = pos.filter(|p| resp.hovered() && !over_gnomon(*p) && !mods.alt);
        self.sk.snap_hit = None;
        self.sk.mouse = mouse;
        let hover = mouse.and_then(|m| self.edit_hit(rect, m));
        if hover.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
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
            if mods.shift {
                if let Some(Hover::Edge(i, _)) = self.edit_hit(rect, m) {
                    if let Some(e) = self.sk.edit.as_mut() {
                        e.edge_sel = Some(i);
                        e.sel = None;
                    }
                } else {
                    self.edit_click(rect, m);
                }
            } else {
                self.edit_click(rect, m);
            }
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
        if self.picking_now() {
            self.paint_pick(p, rect);
            return;
        }
        let f = self.sk.frame;
        self.paint_grid(p, rect, &f);
        let st = &self.sk;
        match st.stage {
            Stage::Ready => self.paint_ready(p, rect),
            Stage::Shape => self.paint_live(p, rect),
            Stage::LoftTop | Stage::SweepPath | Stage::Hole => {
                self.paint_outline(p, rect, &st.pframe, &st.profile, Color32::from_white_alpha(110), 1.3);
                for h in &st.holes {
                    self.paint_outline(p, rect, &st.pframe, h, style::CUT, 1.6);
                }
                self.paint_saved_path(p, rect);
                self.paint_draft(p, rect);
            }
            Stage::Draw => {
                self.paint_saved_path(p, rect);
                self.paint_draft(p, rect);
            }
        }
        if !matches!(self.sk.stage, Stage::Ready | Stage::Shape) {
            self.paint_snap_marker(p, rect, &self.sk.frame);
        }
        if let Some(d) = &st.dimension
            && let Some(q) = d.position(&st.pts)
            && let Some(px) = self.sketch_px(rect, &st.frame, q)
        {
            tag(p, px + vec2(16.0, -27.0), &d.readout());
        }
    }

    fn paint_snap_marker(&self, p: &Painter, rect: Rect, frame: &Frame) {
        let (Some(hit), Some(mouse)) = (self.sk.snap_hit, self.sk.mouse) else { return };
        let Some(at) = self.sketch_px(rect, frame, hit.at) else { return };
        let ink = Color32::from_rgb(255, 215, 100);
        let stroke = Stroke::new(1.8, ink);
        match hit.kind {
            SnapKind::Endpoint | SnapKind::Origin => {
                p.rect_stroke(Rect::from_center_size(at, vec2(9.0, 9.0)), 0.0, stroke, egui::StrokeKind::Middle);
            }
            SnapKind::Midpoint | SnapKind::Center => {
                p.circle_stroke(at, 5.0, stroke);
            }
            SnapKind::Intersection => {
                p.line_segment([at - vec2(5.0, 5.0), at + vec2(5.0, 5.0)], stroke);
                p.line_segment([at + vec2(5.0, -5.0), at + vec2(-5.0, 5.0)], stroke);
            }
            _ => {
                p.circle_stroke(at, 4.0, stroke);
            }
        }
        tag(p, mouse + vec2(15.0, -27.0), hit.kind.label());
    }

    /// A polyline on a frame with a dark halo, so it reads on any background.
    fn paint_outline(&self, p: &Painter, rect: Rect, f: &Frame, pts: &[[f32; 2]], color: Color32, w: f32) {
        self.paint_poly(p, rect, f, pts, true, color, w);
    }

    fn paint_poly(&self, p: &Painter, rect: Rect, f: &Frame, pts: &[[f32; 2]], closed: bool, color: Color32, w: f32) {
        let px: Vec<Pos2> = pts.iter().filter_map(|q| self.sketch_px(rect, f, *q)).collect();
        halo_line(p, px, closed && pts.len() >= 3, color, w);
    }

    pub(crate) fn paint_world_poly(&self, p: &Painter, rect: Rect, pts: &[V3], closed: bool, color: Color32, w: f32) {
        let px: Vec<Pos2> = pts.iter().filter_map(|q| self.cam.project(rect, *q)).collect();
        halo_line(p, px, closed, color, w);
    }

    pub(crate) fn paint_saved_path(&self, p: &Painter, rect: Rect) {
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
        // a corner that misses the plane (looking along it, or far away) must not shrink the grid to a
        // patch: always cover at least what is within the camera's distance of the view's middle
        let reach = if self.cam.dist.is_finite() { self.cam.dist.clamp(50.0, 4000.0) } else { 150.0 };
        if hits < 5 {
            for i in 0..2 {
                lo[i] = lo[i].min(centre[i] - reach);
                hi[i] = hi[i].max(centre[i] + reach);
            }
        }
        // but never more than a few screenfuls around the view's middle
        let cap = (reach * 2.5).max(800.0);
        for i in 0..2 {
            lo[i] = lo[i].max(centre[i] - cap).max(-sketch::COORD_MAX);
            hi[i] = hi[i].min(centre[i] + cap).min(sketch::COORD_MAX);
        }
        // a plane picked on a face: a square around the picked point (visible at any distance)
        if self.sk.on && self.sk.plane_square && self.sk.plane == PlaneSel::Custom && f.origin == self.sk.custom_frame.origin {
            let half = (reach * 0.8).clamp(40.0, 1500.0);
            lo = [-half; 2];
            hi = [half; 2];
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
        self.paint_plane_axes(p, rect, f);
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
                            format!("Radius {} mm", fmt((a[0] - c[0]).hypot(a[1] - c[1])))
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
        for h in &st.holes {
            self.paint_outline(p, rect, f, h, style::CUT, 2.0);
        }
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
                for hole in &st.holes {
                    let top: Vec<V3> = hole.iter().map(|q| crate::sdf::add(f.to_world(*q), up(h))).collect();
                    self.paint_world_poly(p, rect, &top, true, style::CUT.gamma_multiply(0.6), 1.3);
                    rails(hole, hole, up(h));
                }
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
        let ol = e.drag.as_ref().map(|d| &d.candidate).unwrap_or_else(|| sketch::outline_of(sh, e.which));
        self.paint_poly(p, rect, &f, ol, true, style::ACCENT, 2.0);
        if let Some(Ok(preview)) = self.edit_preview() {
            self.paint_poly(p, rect, &f, &preview, true, Color32::from_rgb(90, 228, 160), 2.8);
        }
        self.paint_snap_marker(p, rect, &f);
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
            ui.label(egui::RichText::new("Edit Sketch").strong().color(style::ACCENT));
            if style::pill(ui, "Done", false).on_hover_text("Finish editing (Enter / Esc)").clicked() {
                done = true;
            }
        });
        if loft {
            let cur = self.sk.edit.as_ref().map(|e| e.which);
            ui.horizontal(|ui| {
                for (w, label) in [(Which::Main, "Bottom outline"), (Which::Top, "Top outline")] {
                    if style::pill(ui, label, cur == Some(w)).clicked()
                        && let Some(e) = self.sk.edit.as_mut()
                    {
                        // Cancel an unfinished drag rather than committing an unexpected point.
                        e.which = w;
                        e.sel = None;
                        e.edge_sel = None;
                        e.drag = None;
                        let frame = self.scene.shape(e.g, e.s).and_then(|s| sketch::edit_frame(s, w));
                        if let Some(f) = frame {
                            self.sketch_fly(&f);
                        }
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Edit:");
            for op in [EditOp::Trim, EditOp::Offset, EditOp::Fillet] {
                if style::pill(ui, op.label(), self.sk.edit_op == Some(op)).clicked() {
                    self.sk.edit_op = if self.sk.edit_op == Some(op) { None } else { Some(op) };
                    self.sk.edit_index = None;
                    self.sk.msg.clear();
                }
            }
        });
        if let Some(op) = self.sk.edit_op {
            if op != EditOp::Trim {
                ui.horizontal(|ui| {
                    ui.label(if op == EditOp::Offset { "Distance (mm, negative = inward)" } else { "Radius (mm)" });
                    ui.add(egui::DragValue::new(&mut self.sk.edit_value).speed(0.1));
                });
            }
            hint(
                ui,
                match op {
                    EditOp::Trim => "Click a short edge to remove. Neighbor edges extend to keep the outline closed",
                    EditOp::Offset => "A green preview shows the offset. The original solid is kept",
                    EditOp::Fillet => "Click a vertex to round. Check the green arc",
                },
            );
            if let Some(Err(err)) = self.edit_preview() {
                hint(ui, err.message());
            }
            ui.horizontal(|ui| {
                if style::pill_enabled(ui, "Apply", false, self.edit_preview().is_some_and(|p| p.is_ok())).clicked() {
                    self.edit_commit_operation();
                }
                if style::pill(ui, "Cancel", false).clicked() {
                    self.sk.edit_op = None;
                    self.sk.edit_index = None;
                }
            });
        }
        let n = self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s).map(|s| sketch::outline_of(s, e.which).len())).unwrap_or(0);
        hint(
            ui,
            &format!("Drag point: move  ·  Click line: add point  ·  Click point, Delete: remove  ·  Right-click: done  ·  {n}/{MAX_OUTLINE} points"),
        );
        hint(ui, "1 mm steps  ·  Shift = 10 mm  ·  Ctrl = 0.1 mm  ·  Shift+click line: set length  ·  Alt+drag: orbit");
        // Values are transient UI inputs. Geometry is changed only after a validated edit.
        let current =
            self.sk.edit.as_ref().and_then(|e| self.scene.shape(e.g, e.s).map(|s| (sketch::outline_of(s, e.which).clone(), e.sel, e.edge_sel)));
        if let Some((outline, selected, edge)) = current {
            if let Some(i) = selected.filter(|i| *i < outline.len()) {
                let mut x = outline[i][0];
                let mut y = outline[i][1];
                ui.horizontal(|ui| {
                    ui.label("Point X / Y (mm)");
                    let changed_x = ui.add(egui::DragValue::new(&mut x).speed(0.1).max_decimals(3)).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut y).speed(0.1).max_decimals(3)).changed();
                    if changed_x || changed_y {
                        self.precision_vertex(i, [x, y]);
                    }
                });
            }
            if let Some(size) = sketch::rectangle_size(&outline) {
                let mut w = size[0];
                let mut h = size[1];
                ui.horizontal(|ui| {
                    ui.label("Width / Height (mm)");
                    let cw = ui.add(egui::DragValue::new(&mut w).speed(0.1).range(0.5..=2000.0).max_decimals(3)).changed();
                    let ch = ui.add(egui::DragValue::new(&mut h).speed(0.1).range(0.5..=2000.0).max_decimals(3)).changed();
                    if cw || ch {
                        self.precision_rectangle(w, h);
                    }
                });
            } else if let Some(diameter) = sketch::circle_diameter(&outline) {
                let mut d = diameter;
                ui.horizontal(|ui| {
                    ui.label("Diameter (mm)");
                    if ui.add(egui::DragValue::new(&mut d).speed(0.1).range(1.0..=2000.0).max_decimals(3)).changed() {
                        self.precision_circle(d);
                    }
                });
            }
            if let Some(i) = edge.filter(|i| *i < outline.len()) {
                let p = outline[i];
                let q = outline[(i + 1) % outline.len()];
                let mut length = (q[0] - p[0]).hypot(q[1] - p[1]);
                ui.horizontal(|ui| {
                    ui.label("Segment length (mm)");
                    if ui.add(egui::DragValue::new(&mut length).speed(0.1).range(0.5..=2000.0).max_decimals(3)).changed() {
                        self.precision_edge_length(i, length);
                    }
                });
            }
        }

        if !self.sk.msg.is_empty() {
            ui.label(egui::RichText::new(&self.sk.msg).size(11.5).color(style::CUT));
        }
        if done {
            self.edit_release();
            self.sk.edit = None;
            self.sk.msg.clear();
        }
    }

    fn draw_card_body(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut off = false;
        let stage = self.sk.stage;
        if stage == Stage::Shape && self.sk.live.is_some() {
            self.shape_card(ui, ctx);
            return;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Sketch").strong().color(style::ACCENT));
            if style::pill(ui, "Exit", false).on_hover_text("Exit sketch (Esc)").clicked() {
                off = true;
            }
        });
        // the plane: one small icon, the choices behind it
        let plane_free = matches!(stage, Stage::Draw | Stage::SweepPath);
        if plane_free {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                let cur = self.sk.plane;
                // the quick planes, then "pick on screen": one click each
                let mut quick = None;
                for pl in [PlaneSel::Floor, PlaneSel::Front, PlaneSel::Side, PlaneSel::Camera] {
                    let on = cur == pl && !self.sk.picking;
                    if style::wide_button(ui, pl.label(), pl.tip(), on, None, |p, r, ink| crate::sketch_icons::plane(p, r, pl, ink)).clicked() {
                        quick = Some(pl);
                    }
                }
                if style::wide_button(ui, "Pick plane", "Click a face of a shape or the ground to draw on it", self.sk.picking, None, crate::sketch_icons::pick_plane)
                    .clicked()
                {
                    self.sketch_repick();
                }
                if let Some(pl) = quick {
                    self.sketch_set_plane(pl);
                }
                let r = style::wide_button(ui, "More planes", "More planes: on the selected shape, at a height, custom", false, None, |p, r, ink| {
                    crate::sketch_icons::plane(p, r, PlaneSel::Custom, ink)
                });
                let mut picked = None;
                let mut head_on = false;
                let mut copy = false;
                egui::Popup::menu(&r).show(|ui| {
                    ui.set_min_width(190.0);
                    for pl in PlaneSel::ALL {
                        let usable = pl != PlaneSel::OnShape || self.sel.is_some();
                        let row = ui.add_enabled_ui(usable, |ui| {
                            ui.horizontal(|ui| {
                                let (rc, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                crate::sketch_icons::plane(ui.painter(), rc, pl, ui.visuals().text_color());
                                ui.selectable_label(cur == pl, pl.label()).on_hover_text(pl.tip())
                            })
                            .inner
                        });
                        if row.inner.clicked() {
                            picked = Some(pl);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.selectable_label(false, "View this plane head-on").clicked() {
                        head_on = true;
                        ui.close();
                    }
                    if ui.selectable_label(false, "Copy this plane to custom plane").clicked() {
                        copy = true;
                        ui.close();
                    }
                });
                if let Some(pl) = picked {
                    self.sketch_set_plane(pl);
                }
                if head_on {
                    let f = self.sk.frame;
                    self.sketch_fly(&f);
                }
                if copy {
                    self.sketch_copy_plane_to_custom();
                }
            });
        }
        if plane_free && self.picking_now() {
            hint(ui, "Click a face of a shape, or the ground, to start drawing there");
        }
        if plane_free && matches!(self.sk.plane, PlaneSel::Height | PlaneSel::Custom) {
            self.sketch_plane_settings(ui);
        }
        // the pens
        if matches!(stage, Stage::Draw | Stage::LoftTop | Stage::Hole) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for pen in Pen::ALL {
                    let usable = !((stage == Stage::LoftTop || stage == Stage::Hole) && pen == Pen::Line);
                    let on = self.sk.pen == pen;
                    let tip = format!("{}\n{}", pen.tip(), pen_hint(pen, stage));
                    let r = ui.add_enabled_ui(usable, |ui| style::wide_button(ui, pen.label(), &tip, on, None, |p, r, ink| crate::sketch_icons::pen_tool(p, r, pen, ink))).inner;
                    if r.clicked() {
                        self.sk.pen = pen;
                        self.sk.clear_drawing();
                    }
                }
            });
        }
        if matches!(stage, Stage::Draw | Stage::LoftTop | Stage::SweepPath | Stage::Hole) {
            let pen = self.sk.active_pen();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                if matches!(pen, Pen::Spline | Pen::Line) {
                    let on = self.sk.smooth;
                    let tip = if on { "Smooth curve through the points (click for straight lines)" } else { "Straight lines between points (click for a smooth curve)" };
                    if style::wide_button(ui, if on { "Smooth" } else { "Sharp" }, tip, false, None, |p, r, ink| crate::sketch_icons::smooth(p, r, on, ink)).clicked() {
                        self.sk.smooth = !self.sk.smooth;
                    }
                }
                let snap = self.sk.angle_snap;
                let tip = "Snap angles to 15°. Also snaps to endpoints, midpoints and crossings\nShift = 10 mm  ·  Ctrl = 0.1 mm\nBackspace: undo last point  ·  Esc: restart\nTab: type a length";
                if style::wide_button(ui, "Angle snap 15°", tip, snap, None, crate::sketch_icons::snap).clicked() {
                    self.sk.angle_snap = !self.sk.angle_snap;
                }
            });
        }
        self.dimension_card(ui);
        if stage == Stage::Ready {
            ui.horizontal_wrapped(|ui| {
                hint(ui, &format!("Holes: {} / {}", self.sk.holes.len(), sketch::MAX_HOLES));
                if style::pill_enabled(ui, "+ Draw hole", false, self.sk.holes.len() < sketch::MAX_HOLES).clicked() {
                    self.sk.make = Make::Extrude;
                    if self.sk.pen == Pen::Line {
                        self.sk.pen = Pen::Polygon;
                    }
                    self.sk.stage = Stage::Hole;
                    self.sk.clear_drawing();
                }
                if !self.sk.holes.is_empty() && style::pill(ui, "Remove last hole", false).clicked() {
                    self.sk.holes.pop();
                    self.sk.msg.clear();
                }
            });
            self.make_card(ui, ctx);
        }
        if !self.sk.msg.is_empty() {
            ui.label(egui::RichText::new(&self.sk.msg).size(11.5).color(style::CUT));
        }
        if off {
            self.set_sketch_tool(false);
        }
    }

    /// Numeric positioning and orientation for the two user-defined plane modes.
    fn sketch_plane_settings(&mut self, ui: &mut Ui) {
        match self.sk.plane {
            PlaneSel::Height => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Horizontal plane height Z").size(11.0).color(style::DIM));
                    let mut z = self.sk.level_z;
                    if ui.add(egui::DragValue::new(&mut z).speed(0.25).range(-5000.0..=5000.0).suffix(" mm")).changed() {
                        self.sketch_set_level(z);
                    }
                });
            }
            PlaneSel::Custom => {
                let mut point = self.sk.custom_frame.origin;
                let mut point_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Through point").size(11.0).color(style::DIM));
                    for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.label(*label);
                        point_changed |= ui.add(egui::DragValue::new(&mut point[axis]).speed(0.25).range(-5000.0..=5000.0).suffix(" mm")).changed();
                    }
                });
                if point_changed {
                    self.sketch_edit_custom_plane(point, self.sk.custom_angles);
                }
                let mut angles = self.sk.custom_angles;
                let mut angle_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Rotation").size(11.0).color(style::DIM));
                    for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.label(*label);
                        angle_changed |= ui.add(egui::DragValue::new(&mut angles[axis]).speed(0.25).range(-360.0..=360.0).suffix("°")).changed();
                    }
                });
                if angle_changed {
                    self.sketch_edit_custom_plane(self.sk.custom_frame.origin, angles);
                }
                ui.horizontal_wrapped(|ui| {
                    if style::pill(ui, "Camera target", false)
                        .on_hover_text("Keep this plane's orientation, pass through the camera target")
                        .clicked()
                    {
                        self.sketch_edit_custom_plane(self.cam.target, self.sk.custom_angles);
                    }
                    let chosen = crate::view::targets(&self.scene, self.sel).into_iter().find_map(|(g, s)| self.scene.shape(g, s).map(|sh| sh.pos));
                    if style::pill_enabled(ui, "Selected shape center", false, chosen.is_some())
                        .on_hover_text("Keep this plane's orientation, pass through the selected shape's center")
                        .clicked()
                        && let Some(origin) = chosen
                    {
                        self.sketch_edit_custom_plane(origin, self.sk.custom_angles);
                    }
                });
                hint(ui, "2D coordinates are kept while changing position or angle; the whole sketch moves with the plane");
            }
            _ => {
                if style::pill(ui, "Copy this plane to custom plane", false)
                    .on_hover_text("Duplicate the current plane as a custom plane that can be moved and rotated")
                    .clicked()
                {
                    self.sketch_copy_plane_to_custom();
                }
                if self.sk.plane == PlaneSel::Camera {
                    hint(ui, "This plane stays fixed when the camera orbits. Press Parallel to camera again for a new orientation");
                }
            }
        }
    }

    /// Inline numeric fields; their edits change the visible sketch immediately.
    fn dimension_card(&mut self, ui: &mut Ui) {
        let Some(d) = self.sk.dimension.as_mut() else {
            return;
        };
        if !d.editing {
            if style::pill(ui, "Re-enter dimensions (Tab)", false).clicked() {
                d.begin_again();
            }
            return;
        }
        ui.separator();
        let labels = d.labels();
        let units = d.units();
        let mut changed = None;
        let mut focus = None;
        ui.horizontal(|ui| {
            for index in 0..2 {
                ui.label(labels[index]);
                let id = ui.make_persistent_id(("sketch_dimension", index));
                let response = ui.add(egui::TextEdit::singleline(&mut d.fields[index]).id(id).desired_width(75.0));
                if response.changed() {
                    changed = Some(index);
                }
                if d.needs_focus && d.selected == index {
                    focus = Some(id);
                }
                ui.label(units[index]);
            }
        });
        if let Some(id) = focus {
            ui.memory_mut(|m| m.request_focus(id));
            d.needs_focus = false;
        }
        if let Some(index) = changed {
            d.selected = index;
            let profile = if d.top_profile { &mut self.sk.profile2 } else { &mut self.sk.profile };
            let prevent_cross = matches!(self.sk.pen, Pen::Polygon | Pen::Spline) && !self.sk.smooth;
            d.update(&mut self.sk.pts, profile, index, prevent_cross);
            ui.ctx().request_repaint();
        }
        if let Some(error) = &d.error {
            ui.colored_label(style::CUT, error);
        }
        hint(ui, "Tab: next field  ·  Enter: confirm  ·  Esc: cancel");
    }

    /// The part of the card that asks what to make.
    fn make_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let vertical = self.sk.pframe.is_vertical();
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Create").size(11.0).color(style::DIM));
            for m in Make::ALL {
                let usable = (m != Make::Revolve || vertical) && (self.sk.holes.is_empty() || m == Make::Extrude);
                let tip = match m {
                    Make::Extrude => "Extrude the outline straight into a prism",
                    Make::Revolve if vertical => "Revolve the outline around the vertical center axis to make a vase-like shape",
                    Make::Revolve => "Revolve works on sketches drawn on the Front or Side plane",
                    Make::Loft => "Blend smoothly from the bottom outline to the top outline",
                    Make::Sweep => "Pull the outline along the drawn path",
                };
                if style::pill_enabled(ui, m.label(), self.sk.make == m, usable).on_hover_text(tip).clicked() {
                    self.sk.make = m;
                    self.sk.msg.clear();
                }
            }
        });
        let make = self.sk.make;
        if matches!(make, Make::Extrude | Make::Loft) {
            style::labeled_slider(ui, "Height", &mut self.sk.height, 0.5..=300.0, "Extrude height (mm)");
            ui.horizontal(|ui| {
                if style::pill(ui, "Flip", self.sk.flip).on_hover_text("Extrude to the opposite side of the plane").clicked() {
                    self.sk.flip = !self.sk.flip;
                }
            });
        }
        if make == Make::Loft {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Top outline").size(11.0).color(style::DIM));
                if style::pill(ui, "Scaled copy", self.sk.loft_top == LoftTop::Scaled).clicked() {
                    self.sk.loft_top = LoftTop::Scaled;
                }
                if style::pill(ui, "Draw another", self.sk.loft_top == LoftTop::Drawn).clicked() {
                    self.sk.loft_top = LoftTop::Drawn;
                    if self.sk.profile2.is_empty() {
                        self.sk.stage = Stage::LoftTop;
                        self.sk.clear_drawing();
                    }
                }
            });
            if self.sk.loft_top == LoftTop::Scaled {
                style::labeled_slider(ui, "Top scale", &mut self.sk.loft_scale, 0.1..=3.0, "Top outline size (ratio, bottom = 1)");
            } else if style::pill(ui, if self.sk.profile2.is_empty() { "Draw top outline" } else { "Redraw top outline" }, false).clicked() {
                self.sk.stage = Stage::LoftTop;
                self.sk.profile2.clear();
                self.sk.clear_drawing();
            }
        }
        if make == Make::Sweep {
            style::labeled_slider(ui, "Thickness", &mut self.sk.sweep_scale, 0.2..=4.0, "Outline size scale");
            ui.horizontal(|ui| {
                let have = !self.sk.path.is_empty();
                ui.label(
                    egui::RichText::new(if have { format!("Path: {} points", self.sk.path.len()) } else { "No path yet".to_string() })
                        .size(11.5)
                        .color(style::DIM),
                );
                if style::pill(ui, if have { "Redraw path" } else { "Draw path" }, false).on_hover_text("Draw the path (Enter to finish)").clicked() {
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
            let b = egui::Button::new(egui::RichText::new(format!("Make {}", make.label())).strong().color(Color32::WHITE))
                .fill(style::ACCENT_SOFT)
                .stroke(Stroke::new(1.0, style::ACCENT));
            if ui.add_enabled(ready, b).clicked() {
                self.sketch_make(ctx);
            }
            if style::pill(ui, "Redo", false).on_hover_text("Discard the outline and draw again (Esc)").clicked() {
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
        (_, Stage::LoftTop) if pen != Pen::Line => "Drawing the loft's top outline (bottom outline shown faintly)",
        (Pen::Polygon, _) => "Left-click: place corner  ·  Right-click / first point / Enter: close",
        (Pen::Rect, _) => "Drag (or click twice) to set opposite corners",
        (Pen::Circle, _) => "Drag from center outward (or click twice) to set radius",
        (Pen::Spline, _) => "Left-click: smooth curve through points  ·  Right-click / first point / Enter: close",
        (Pen::Line, Stage::SweepPath) => "Drawing sweep path  ·  Left-click: point  ·  Right-click / Enter: finish",
        (Pen::Line, _) => "Left-click: place point  ·  Right-click / Enter: finish (becomes sweep path)",
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
pub(crate) fn tag(p: &Painter, at: Pos2, text: &str) {
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

    /// Finish a gesture the way the old card flow did: the outline stays "Ready" (no live solid),
    /// for tests that set the height or the holes before making the shape.
    fn done_ready(app: &mut App, d: Finished) {
        app.sketch_done(d);
        app.live_to_ready();
    }

    #[test]
    fn precise_edits_are_rejected_atomically_and_support_undo_redo() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        let profile = app.sk.click([20.0, 10.0], false).unwrap();
        app.sketch_done(profile);
        assert!(app.sketch_make(&ctx));
        app.settle(&ctx);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        app.sketch_edit_begin(g, s);
        let before = app.scene.shape(g, s).unwrap().outline.clone();
        assert!(app.precision_rectangle(40.0, 15.0));
        app.settle(&ctx);
        assert_eq!(sketch::rectangle_size(&app.scene.shape(g, s).unwrap().outline), Some([40.0, 15.0]));
        let good = app.scene.shape(g, s).unwrap().outline.clone();
        assert!(!app.precision_vertex(2, good[0]));
        assert_eq!(app.scene.shape(g, s).unwrap().outline, good);
        app.undo();
        assert_eq!(app.scene.shape(g, s).unwrap().outline, before);
        app.redo();
        assert_eq!(app.scene.shape(g, s).unwrap().outline, good);
        assert!(app.precision_edge_length(0, 42.0));
        assert!(sketch::validate_profile(&app.scene.shape(g, s).unwrap().outline).is_ok());
        let saved = app.scene.to_json();
        assert_eq!(Scene::from_json(&saved).unwrap().shape(g, s).unwrap().outline, app.scene.shape(g, s).unwrap().outline);
    }

    #[test]
    fn a_polygon_is_closed_by_clicking_the_first_point_or_enter() {
        let mut st = State::default();
        st.pen = Pen::Polygon;
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
        let mut st = State::default();
        st.pen = Pen::Polygon;
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
        let mut bow = State::default();
        bow.pts = vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 15.0]];
        assert!(bow.enter().is_none());
        assert_eq!(bow.msg, SketchError::SelfIntersect.message());
    }

    #[test]
    fn rectangles_and_circles_are_dragged_or_clicked_out() {
        let mut st = State::default();
        st.pen = Pen::Rect;
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
        let mut st = State::default();
        st.pen = Pen::Spline;
        st.smooth = true;
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
        let mut st = State::default();
        st.pen = Pen::Polygon;
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

    #[test]
    fn multiple_holes_create_a_boolean_group_and_survive_file_roundtrip() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.scene.groups.clear();
        app.set_sketch_tool(true);
        done_ready(&mut app, Finished::Profile(sketch::rect([-24.0, -20.0], [24.0, 20.0])));
        app.sk.stage = Stage::Hole;
        done_ready(&mut app, Finished::Profile(sketch::circle([-11.0, 0.0], 5.0, sketch::CIRCLE_POINTS)));
        app.sk.stage = Stage::Hole;
        done_ready(&mut app, Finished::Profile(sketch::rect([8.0, -6.0], [16.0, 6.0])));
        assert_eq!(app.sk.holes.len(), 2);
        app.sk.height = 8.0;
        app.sk.make = Make::Extrude;
        assert!(app.sketch_make(&ctx), "{}", app.sk.msg);
        assert_eq!(app.scene.groups.len(), 1);
        let g = &app.scene.groups[0];
        assert_eq!(g.shapes.len(), 3);
        assert!(g.sketch_holes);
        assert_eq!(g.shapes[0].op, Op::Add);
        assert!(g.shapes[1..].iter().all(|s| s.op == Op::Subtract && s.blend == 0.0));
        let field = crate::sdf::Field::new(&app.scene);
        assert!(field.distance([0.0, 0.0, 4.0]) < 0.0);
        assert!(field.distance([-11.0, 0.0, 4.0]) > 0.0);
        assert!(field.distance([12.0, 0.0, 4.0]) > 0.0);
        let back = crate::model::Scene::from_json(&app.scene.to_json()).expect("saved scene");
        assert_eq!(back.groups[0].shapes.len(), 3);
        assert!(crate::sdf::Field::new(&back).distance([12.0, 0.0, 4.0]) > 0.0);
    }

    #[test]
    fn concentric_eccentric_rectangular_multiple_holes_export_as_watertight_stl() {
        use crate::export_formats::Format;
        use crate::export_job::{self, Options, Scope};
        use crate::export_mesh::Progress;
        use solidcraft_geom::melt;
        let ctx = egui::Context::default();
        let mut app = app();
        app.scene.groups.clear();
        let mut scenarios: Vec<Vec<Vec<[f32; 2]>>> = vec![
            vec![sketch::circle([0.0, 0.0], 8.0, sketch::CIRCLE_POINTS)],
            vec![sketch::circle([8.0, 0.0], 6.0, sketch::CIRCLE_POINTS)],
            vec![sketch::rect([-10.0, -6.0], [10.0, 6.0])],
            vec![sketch::circle([-11.0, 0.0], 5.0, sketch::CIRCLE_POINTS), sketch::rect([8.0, -6.0], [16.0, 6.0])],
        ];
        for (i, holes) in scenarios.drain(..).enumerate() {
            app.scene.groups.clear();
            app.sk.reset();
            app.sk.on = true;
            done_ready(&mut app, Finished::Profile(sketch::rect([-25.0, -20.0], [25.0, 20.0])));
            app.sk.holes = holes.clone();
            app.sk.make = Make::Extrude;
            app.sk.height = 8.0;
            assert!(app.sketch_make(&ctx), "scenario {i}: {}", app.sk.msg);
            let options = Options { format: Format::Stl, tolerance: 0.45, scope: Scope::Visible, ..Options::default() };
            let built = export_job::build(&app.scene, None, &options, &Progress::new()).expect("export mesh");
            assert!(built.closed, "scenario {i}");
            assert!(melt::is_watertight(&built.surface.mesh), "scenario {i}");
            let area = 50.0 * 40.0 - holes.iter().map(|h| sketch::signed_area(h).abs()).sum::<f32>();
            let expected = f64::from(area * 8.0);
            let measured = melt::volume(&built.surface.mesh).abs();
            assert!((measured / expected - 1.0).abs() < 0.04, "scenario {i}: {measured} vs {expected}");
            let bytes = export_job::serialize(&built, &options, std::path::Path::new("test-holes.stl")).expect("STL serialization");
            assert_eq!(bytes.len(), 1);
            assert!(bytes[0].1.len() > 84, "non-empty binary STL scenario {i}");
        }
    }

    #[test]
    fn touching_hole_is_refused_in_sketch_card() {
        let mut app = app();
        app.set_sketch_tool(true);
        app.sketch_done(Finished::Profile(sketch::rect([-20.0, -20.0], [20.0, 20.0])));
        app.sk.stage = Stage::Hole;
        app.sketch_done(Finished::Profile(sketch::rect([20.0, -5.0], [25.0, 5.0])));
        assert!(app.sk.holes.is_empty());
        assert!(!app.sk.msg.is_empty());
    }

    /// Draw a profile on the floor and make every kind of solid of it, through the same calls the card uses.
    #[test]
    fn a_profile_becomes_each_kind_of_solid_in_the_right_group() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.sel = None;
        app.set_sketch_tool(true);
        assert!(app.sk.on && app.picking_now(), "the viewport waits for a plane to be picked");
        let n = app.scene.shape_count();
        app.sk.pen = Pen::Rect;
        app.sk.click([10.0, 10.0], false);
        let p = app.sk.click([40.0, 30.0], false);
        done_ready(&mut app, p.expect("a profile"));
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
        done_ready(&mut app, p);
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
        done_ready(&mut app, c);
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
        done_ready(&mut app, c);
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
        done_ready(&mut app, l);
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
        done_ready(&mut app, p);
        app.sk.make = Make::Loft;
        app.sk.loft_top = LoftTop::Drawn;
        app.sk.stage = Stage::LoftTop;
        assert_eq!(app.sk.frame, app.sk.pframe);
        app.sk.pen = Pen::Circle;
        app.sk.click([10.0, 10.0], false);
        let c = app.sk.click([15.0, 10.0], false).expect("circle");
        done_ready(&mut app, c);
        assert_eq!((app.sk.stage, app.sk.profile2.len()), (Stage::Ready, sketch::CIRCLE_POINTS));
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let l = app.scene.shape(g, s).expect("there");
        assert_eq!((l.kind, l.outline.len(), l.outline2.len()), (Kind::Loft, 4, sketch::CIRCLE_POINTS));
    }

    #[test]
    fn height_and_custom_planes_can_be_reselected_without_coordinate_drift() {
        let mut app = app();
        app.sel = None;
        app.set_sketch_tool(true);
        assert_eq!(app.sketch_plane(), Frame::floor());
        app.sketch_set_plane(PlaneSel::Height);
        assert!(app.sketch_set_level(84.25));
        assert_eq!(app.sk.frame, Frame::horizontal(84.25));
        assert!(!app.sketch_set_level(f32::NAN));
        assert_eq!(app.sk.frame, Frame::horizontal(84.25));
        app.sketch_set_plane(PlaneSel::Side);
        assert_eq!(app.sk.frame, Frame::side());
        app.sketch_set_plane(PlaneSel::Height);
        assert_eq!(app.sk.frame, Frame::horizontal(84.25), "height survives selection changes");
        app.sketch_set_plane(PlaneSel::Front);
        app.sketch_copy_plane_to_custom();
        assert_eq!(app.sk.plane, PlaneSel::Custom);
        assert_eq!(app.sk.frame, Frame::front(), "copy keeps the original basis");

        let angles = [23.0, -34.0, 58.0];
        assert!(app.sketch_edit_custom_plane([12.0, -7.0, 31.0], angles));
        let before_world = app.sk.frame.to_world([4.0, -6.0]);
        app.sk.pts = vec![[4.0, -6.0], [10.0, 13.0]];
        let original_points = app.sk.pts.clone();
        assert!(app.sketch_edit_custom_plane([12.0, -7.0, 44.0], angles));
        assert_eq!(app.sk.pts, original_points, "moving plane preserves local draft values");
        let saved_frame = app.sk.frame;
        let after_world = saved_frame.to_world([4.0, -6.0]);
        assert!((after_world[2] - before_world[2] - 13.0).abs() < 1e-4);
        app.sketch_set_plane(PlaneSel::Floor);
        assert!(app.sk.pts.is_empty(), "switching presets discards only the unfinished gesture");
        assert!(app.sk.msg.contains("unfinished"));
        app.sketch_set_plane(PlaneSel::Custom);
        assert_eq!(app.sk.frame, saved_frame, "custom plane is reselectable");
        assert!(!app.sketch_edit_custom_plane([f32::NAN, 0.0, 0.0], angles));
        assert_eq!(app.sk.frame, saved_frame, "invalid edits leave the last valid plane intact");
    }

    #[test]
    fn camera_parallel_plane_is_a_fixed_snapshot_with_inverse_ray_mapping() {
        let mut app = app();
        app.set_sketch_tool(true);
        for (yaw, pitch, ortho) in [(0.0, 0.0, false), (1.0, 0.65, true), (-1.2, -0.43, false), (2.7, std::f32::consts::FRAC_PI_2 - 0.002, true)] {
            app.cam.look(yaw, pitch);
            app.cam.target = [10.0, -20.0, 35.0];
            app.cam.dist = 160.0;
            app.cam.ortho = ortho;
            let (_, _, forward) = app.cam.basis();
            app.sketch_set_plane(PlaneSel::Camera);
            let f = app.sk.frame;
            assert!(app.fly.is_none(), "a previous fly-to must not rotate the camera");
            assert!(crate::sdf::dot(f.normal(), forward) < -0.999, "faces camera: {f:?}");
            for q in [[0.0, 0.0], [7.0, -3.0], [-12.0, 18.0]] {
                let px = app.cam.project(rect(), f.to_world(q)).expect("visible workplane");
                let recovered = app.sketch_cursor(rect(), px, 0.1, &f).expect("hit plane");
                assert!(
                    (recovered[0] - q[0]).abs() < 0.11 && (recovered[1] - q[1]).abs() < 0.11,
                    "camera={yaw},{pitch} ortho={ortho}: {q:?} -> {recovered:?}"
                );
            }
            app.cam.orbit(20.0, -12.0);
            assert_eq!(app.sketch_plane(), f, "orbiting does not silently turn a saved camera plane");
        }
    }

    #[test]
    fn rotated_plane_profile_and_shape_survive_switch_and_save() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sketch_set_plane(PlaneSel::Custom);
        assert!(app.sketch_edit_custom_plane([35.0, -14.0, 63.0], [20.0, -35.0, 52.0]));
        let plane = app.sk.frame;
        app.sk.pen = Pen::Rect;
        assert!(app.sk.click([0.0, 0.0], false).is_none());
        let p = app.sk.click([20.0, 12.0], false).expect("rectangle");
        app.sketch_done(p);
        assert_eq!(app.sk.pframe, plane);
        app.sketch_set_plane(PlaneSel::Floor);
        assert_eq!(app.sk.pframe, plane, "a finished profile owns its original frame");
        app.sk.height = 8.0;
        assert!(app.sketch_make(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected extrusion") };
        let restored = Scene::from_json(&app.scene.to_json()).expect("saved scene loads");
        let solid = restored.shape(g, s).expect("saved extrusion");
        let edited = sketch::edit_frame(solid, Which::Main).expect("re-editable shape");
        let expected = plane.to_world([10.0, 6.0]);
        assert!(
            crate::sdf::len(crate::sdf::sub(edited.to_world([0.0, 0.0]), expected)) < 0.02,
            "saved 2D outline reopens on the correct oblique world plane"
        );
    }

    #[test]
    fn camera_fly_faces_a_slanted_custom_plane() {
        let mut app = app();
        app.sketch_set_plane(PlaneSel::Custom);
        assert!(app.sketch_edit_custom_plane([12.0, 30.0, 18.0], [26.0, -37.0, 64.0]));
        let frame = app.sk.frame;
        app.sketch_fly(&frame);
        for _ in 0..300 {
            app.step_fly(0.016);
        }
        let (_, _, forward) = app.cam.basis();
        assert!(crate::sdf::dot(forward, frame.normal()) < -0.999, "the camera should look straight into any valid plane normal");
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
        done_ready(&mut app, p);
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

    #[test]
    fn screen_space_snap_targets_nearby_existing_vertex_at_different_zoom_levels() {
        let mut app = app();
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        app.cam.target = [0.0, 0.0, 0.0];
        app.sk.on = true;
        app.sk.pen = Pen::Polygon;
        app.sk.pts = vec![[20.0, 10.0], [30.0, 10.0]];
        let r = rect();
        let f = Frame::floor();
        for distance in [90.0, 250.0] {
            app.cam.dist = distance;
            let px = app.cam.project(r, f.to_world([20.0, 10.0])).unwrap();
            let mouse = px + vec2(6.0, 0.0);
            let (point, hit) = app.sketch_resolve_snap(r, mouse, 1.0, &f, None, None).unwrap();
            assert_eq!(point, [20.0, 10.0]);
            assert_eq!(hit.unwrap().kind, SnapKind::Endpoint);
        }
    }

    #[test]
    fn screen_space_snap_keeps_cursor_free_when_no_target_is_close() {
        let mut app = app();
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        app.cam.dist = 100.0;
        app.sk.on = true;
        app.sk.pts = vec![[30.0, 30.0]];
        let r = rect();
        let f = Frame::floor();
        let point = app.cam.project(r, f.to_world([45.2, 55.4])).unwrap();
        let (at, hit) = app.sketch_resolve_snap(r, point, 1.0, &f, None, None).unwrap();
        assert!(at[0].is_finite() && at[1].is_finite());
        assert!(hit.is_none_or(|h| h.kind == SnapKind::Grid));
    }

    /// Edit the outline of a made shape point by point, with undo.
    #[test]
    fn an_outline_is_edited_in_the_viewport_with_undo() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.sel = None;
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Rect;
        app.sk.click([0.0, 0.0], false);
        let p = app.sk.click([20.0, 20.0], false).expect("rect");
        done_ready(&mut app, p);
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
        assert_eq!(app.scene.shape(g, s).expect("there").outline, before, "the heavy 3D shape stays stable during preview");
        let moved = app.sk.edit.as_ref().unwrap().drag.as_ref().unwrap().candidate[2];
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
        assert_ne!(app.sk.edit.as_ref().unwrap().drag.as_ref().unwrap().candidate, good, "the preview follows the mouse");
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
    fn escape_cancels_only_drag_and_preserves_solid_selection() {
        let mut app = app();
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Extrude);
        app.settle(&ctx);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("shape") };
        app.sketch_edit_begin(g, s);
        let original = app.scene.shape(g, s).unwrap().clone();
        assert!(app.edit_begin_drag(2));
        let r = rect();
        let f = sketch::edit_frame(&original, Which::Main).unwrap();
        let at = app.cam.project(r, f.to_world([35.0, 35.0])).unwrap();
        app.edit_drag(r, at, 1.0);
        assert!(app.edit_cancel_drag());
        assert_eq!(app.scene.shape(g, s), Some(&original));
        assert_eq!(app.sel, Some(Pick::Shape(g, s)));
        assert!(app.sk.edit.is_some());
        app.settle(&ctx);
        assert_eq!(app.scene.shape(g, s), Some(&original), "cancel adds no model change");
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
        assert_eq!(app.sk.stage, Stage::Shape, "the closed outline is a live solid");
    }

    #[test]
    fn live_dimensions_survive_save_reload_and_one_undo_redo() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Rect;
        let a = [8.0, 12.0];
        let b = [28.0, 22.0];
        let raw = profile_of(app.sk.click(a, false).or_else(|| app.sk.click(b, false)));
        app.sketch_done(Finished::Profile(raw.clone()));
        app.sk.dimension = Dimensions::for_shape(Pen::Rect, a, b, &raw, false);
        let Some(d) = app.sk.dimension.as_mut() else { panic!("editor missing") };
        d.fields = ["100".into(), "50".into()];
        d.update(&mut app.sk.pts, &mut app.sk.profile, 1, false);
        assert!(d.error.is_none(), "{:?}", d.error);
        assert!(d.accept(&app.sk.pts, &app.sk.profile));
        assert_eq!(sketch::bbox(&app.sk.profile), Some(([8.0, 12.0], [108.0, 62.0])));
        assert!(app.sketch_make(&ctx));
        app.settle(&ctx);
        let Some(Pick::Shape(g, si)) = app.sel else { panic!("shape not selected") };
        let saved = app.scene.to_json();
        let reloaded = Scene::from_json(&saved).expect("valid saved scene");
        let original_outline = app.scene.shape(g, si).map(|s| s.outline.clone()).expect("new shape");
        assert_eq!(reloaded.shape(g, si).map(|s| &s.outline), Some(&original_outline));
        app.undo();
        assert_eq!(app.scene.shape_count(), 1, "one Enter plus make: one scene edit");
        app.redo();
        assert_eq!(app.scene.shape(g, si).map(|s| &s.outline), Some(&original_outline));
    }

    #[test]
    fn tab_enters_point_dimensions_without_global_view_shortcut() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.set_sketch_tool(true);
        app.sk.pen = Pen::Line;
        app.sk.click([0.0, 0.0], false);
        app.sk.click([10.0, 0.0], false);
        let key = |k: egui::Key| {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE })
            })
        };
        key(egui::Key::Tab);
        app.keys(&ctx);
        assert!(app.sk.dimension.as_ref().is_some_and(|d| d.editing));
        assert_eq!(app.sk.dimension.as_ref().map(|d| d.selected), Some(0));
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Tab);
        app.keys(&ctx);
        assert_eq!(app.sk.dimension.as_ref().map(|d| d.selected), Some(1));
        let cam = app.cam;
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Num1);
        app.keys(&ctx);
        assert_eq!(
            (app.cam.yaw, app.cam.pitch, app.cam.ortho),
            (cam.yaw, cam.pitch, cam.ortho),
            "number keys must not switch views while entering dimensions"
        );
        ctx.input_mut(|i| i.events.clear());
        key(egui::Key::Escape);
        app.keys(&ctx);
        assert!(app.sk.on && app.sk.dimension.as_ref().is_some_and(|d| !d.editing));
        assert_eq!(app.sk.pts, vec![[0.0, 0.0], [10.0, 0.0]]);
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
