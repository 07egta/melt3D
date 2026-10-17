//! Editing curves (and sweep paths) in the viewport, the way Womp3D does it: the nodes show as
//! small circles; drag one to move it, drag from the `+` at an end to pull out a new node, click
//! the `+` between two nodes to add one there. Also the "せん" tool that draws a new curve by
//! clicking, and the inspector section for curves.
//!
//! The pure helpers (node lists, projection, hit testing) come first and have unit tests; the
//! `App` methods at the bottom are the thin glue to the mouse, the keys and the panels.

use egui::{Color32, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::camera::Camera;
use crate::model::{CurveHandles, CurveNodeMode, Group, Kind, MAX_PATH, MAX_SHAPES, Material, Pick, Shape};
use crate::sdf::{V3, add, dot, len, mul, norm, rotation, sub};
use crate::{curve, style};

// ---------------------------------------------------------------- node lists (pure)

/// Which end of a curve.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    Start,
    Last,
}

/// Add a node at an end (it takes that end's radius). Returns its index, or None when full.
pub fn extend_end(path: &mut Vec<[f32; 4]>, end: End, pos: V3) -> Option<usize> {
    if path.len() >= MAX_PATH {
        return None;
    }
    let r = match end {
        End::Start => path.first(),
        End::Last => path.last(),
    }
    .map(|p| p[3])
    .unwrap_or(5.0);
    let node = [pos[0], pos[1], pos[2], r];
    match end {
        End::Start => {
            path.insert(0, node);
            Some(0)
        }
        End::Last => {
            path.push(node);
            Some(path.len() - 1)
        }
    }
}

/// Put `at` right after node `after`. Returns the new index, or None when full or out of range.
pub fn insert_node(path: &mut Vec<[f32; 4]>, after: usize, at: [f32; 4]) -> Option<usize> {
    if path.len() >= MAX_PATH || after >= path.len() {
        return None;
    }
    path.insert(after + 1, at);
    Some(after + 1)
}

/// Remove node `i`, keeping at least two. True when something was removed.
pub fn delete_node(path: &mut Vec<[f32; 4]>, i: usize) -> bool {
    if path.len() <= 2 || i >= path.len() {
        return false;
    }
    path.remove(i);
    true
}

pub fn set_all_radii(path: &mut [[f32; 4]], r: f32) {
    let r = if r.is_finite() { r.clamp(0.1, 500.0) } else { 5.0 };
    for p in path {
        p[3] = r;
    }
}

/// Keep the optional handles aligned with nodes as paths are edited.
fn align_handles(shape: &mut Shape) {
    if !shape.curve_handles.is_empty() {
        shape.curve_handles.resize(shape.path.len(), CurveHandles::default());
    }
}

/// Round every number to a multiple of `step`.
pub fn snap(v: V3, step: f32) -> V3 {
    [(v[0] / step).round() * step, (v[1] / step).round() * step, (v[2] / step).round() * step]
}

// ---------------------------------------------------------------- space (pure)

/// A point in the shape's own space, in the world.
pub fn to_world(s: &Shape, p: V3) -> V3 {
    let r = rotation(s.rot);
    let sp = [p[0] * s.scale[0], p[1] * s.scale[1], p[2] * s.scale[2]];
    add(s.pos, [dot(r[0], sp), dot(r[1], sp), dot(r[2], sp)])
}

/// A world point in the shape's own space.
pub fn to_local(s: &Shape, w: V3) -> V3 {
    let r = rotation(s.rot);
    let d = sub(w, s.pos);
    let q = [r[0][0] * d[0] + r[1][0] * d[1] + r[2][0] * d[2], r[0][1] * d[0] + r[1][1] * d[1] + r[2][1] * d[2], r[0][2] * d[0] + r[1][2] * d[1] + r[2][2] * d[2]];
    [q[0] / s.scale[0].max(1e-6), q[1] / s.scale[1].max(1e-6), q[2] / s.scale[2].max(1e-6)]
}

/// Where a ray meets a plane (through `point`, facing `normal`), in front of the ray.
pub fn ray_plane(o: V3, d: V3, point: V3, normal: V3) -> Option<V3> {
    let den = dot(d, normal);
    if den.abs() < 1e-4 {
        return None;
    }
    let t = dot(sub(point, o), normal) / den;
    (t.is_finite() && t > 0.0).then(|| add(o, mul(d, t)))
}

/// The point on a plane under a screen position. The plane faces the camera, or is level with the
/// ground when `level`.
pub fn unproject_on_plane(cam: &Camera, rect: Rect, screen: Pos2, through: V3, level: bool) -> Option<V3> {
    let (o, d) = cam.ray_at(rect, screen);
    let normal = if level { [0.0, 0.0, 1.0] } else { cam.basis().2 };
    ray_plane(o, d, through, normal)
}

/// The size of a node's circle on screen, in points.
pub fn node_px(cam: &Camera, rect: Rect, world: V3, radius_mm: f32) -> f32 {
    (radius_mm / cam.mm_per_px(world, rect.height()).max(1e-6)).clamp(7.0, 90.0)
}

// ---------------------------------------------------------------- what is on screen (pure)

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NodeView {
    pub c: Pos2,
    pub r: f32,
}

/// What the mouse can grab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    /// The circle of node `i`.
    Node(usize),
    /// The dot on the edge of the chosen node's circle: drag to change its radius.
    Radius(usize),
    /// The `+` ring beyond an end: drag to pull out a new node.
    End(End),
    /// The `+` on segment `i` (between node `i` and the next): click to add a node there.
    Mid(usize),
    /// Incoming/outgoing cubic Bezier control point.
    Bezier(usize, bool),
}

/// Where everything of one path sits on screen this frame.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub nodes: Vec<Option<NodeView>>,
    pub mids: Vec<(usize, Pos2)>,
    /// (end, the `+` ring, the end node's centre)
    pub ends: Vec<(End, Pos2, Pos2)>,
    /// The smooth curve, projected.
    pub line: Vec<Pos2>,
    /// (node index, outgoing, centre, handle position)
    pub bezier: Vec<(usize, bool, Pos2, Pos2)>,
}

const END_GAP: f32 = 17.0;
const PLUS_R: f32 = 9.0;
const MID_R: f32 = 6.5;

/// Lay a curve or sweep path out on screen.
pub fn layout(cam: &Camera, rect: Rect, s: &Shape) -> Layout {
    let is_curve = s.kind == Kind::Curve;
    let (closed, smooth, _, thick) = if is_curve { curve::settings(&s.params) } else { (false, 0.0, false, 1.0) };
    let k = s.scale[0].min(s.scale[1]).min(s.scale[2]) * thick;
    let n = s.path.len();
    let mut out = Layout::default();
    for p in &s.path {
        let w = to_world(s, [p[0], p[1], p[2]]);
        out.nodes.push(cam.project(rect, w).map(|c| NodeView { c, r: if is_curve { node_px(cam, rect, w, p[3] * k) } else { 8.0 } }));
    }
    let segs = if closed && n >= 3 { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        let m = curve::segment_point(&s.path, i, 0.5, smooth, closed);
        if let Some(c) = cam.project(rect, to_world(s, [m[0], m[1], m[2]])) {
            out.mids.push((i, c));
        }
    }
    if !(closed && n >= 3) {
        for (end, a, b) in [(End::Start, 0usize, 1usize), (End::Last, n.wrapping_sub(1), n.wrapping_sub(2))] {
            if let (Some(Some(c)), Some(Some(from))) = (out.nodes.get(a), out.nodes.get(b)) {
                let mut dir = c.c - from.c;
                dir = if dir.length() > 1e-3 { dir.normalized() } else { vec2(1.0, 0.0) };
                out.ends.push((end, c.c + dir * (c.r + END_GAP), c.c));
            }
        }
    }
    if is_curve {
        for (i, h) in s.curve_handles.iter().enumerate() {
            if h.mode == CurveNodeMode::Auto { continue; }
            let Some(node) = s.path.get(i) else { continue };
            let centre = [node[0],node[1],node[2]];
            for (outgoing, delta) in [(false,h.incoming),(true,h.outgoing)] {
                let end = add(centre,delta);
                if let (Some(c),Some(p)) = (cam.project(rect,to_world(s,centre)),cam.project(rect,to_world(s,end))) {
                    out.bezier.push((i,outgoing,c,p));
                }
            }
        }
    }
    let pts = if is_curve { curve::build_shape(s) } else { s.path.clone() };
    out.line = pts.iter().filter_map(|p| cam.project(rect, to_world(s, [p[0], p[1], p[2]]))).collect();
    out
}

/// What is under `p`, nearest first: the radius dot, a node, an end ring, a segment `+`.
pub fn hit(l: &Layout, p: Pos2, chosen: Option<usize>) -> Option<Hit> {
    if let Some(i) = chosen
        && let Some(Some(n)) = l.nodes.get(i)
        && (pos2(n.c.x + n.r, n.c.y) - p).length() <= 7.0
    {
        return Some(Hit::Radius(i));
    }
    if let Some(i) = chosen {
        for (idx, outgoing, _, point) in &l.bezier {
            if *idx == i && (*point - p).length() <= 9.0 {
                return Some(Hit::Bezier(i, *outgoing));
            }
        }
    }
    let mut best: Option<(f32, usize)> = None;
    for (i, n) in l.nodes.iter().enumerate() {
        if let Some(n) = n {
            let d = (n.c - p).length();
            if d <= n.r.max(9.0) + 2.0 && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, i));
            }
        }
    }
    if let Some((_, i)) = best {
        return Some(Hit::Node(i));
    }
    for (end, c, _) in &l.ends {
        if (*c - p).length() <= PLUS_R + 3.0 {
            return Some(Hit::End(*end));
        }
    }
    for (i, c) in &l.mids {
        if (*c - p).length() <= MID_R + 3.0 {
            return Some(Hit::Mid(*i));
        }
    }
    None
}

// ---------------------------------------------------------------- state

/// A node being dragged.
#[derive(Clone, Copy, Debug)]
pub struct NodeDrag {
    pub g: usize,
    pub s: usize,
    pub i: usize,
    pub radius: bool,
    pub handle: Option<bool>,
    /// The Shift key state last frame (level with the ground instead of facing the camera).
    pub level: bool,
    /// Node position minus the pointer's place on the plane when grabbed.
    pub grab: V3,
}

pub struct State {
    /// Show and edit the nodes of the selected curve ("ふしをいじる").
    pub edit_nodes: bool,
    /// The chosen node (group, shape, node).
    pub node: Option<(usize, usize, usize)>,
    pub hover: Option<Hit>,
    pub drag: Option<NodeDrag>,
    /// The "せん" tool is on: clicks lay down nodes of a new curve.
    pub drawing: bool,
    /// The curve being drawn.
    pub draw_shape: Option<(usize, usize)>,
    /// Radius of new nodes (mm).
    pub draw_radius: f32,
    /// Where the next node would go (follows the mouse while drawing).
    pub preview: Option<V3>,
}

impl Default for State {
    fn default() -> Self {
        State { edit_nodes: true, node: None, hover: None, drag: None, drawing: false, draw_shape: None, draw_radius: 5.0, preview: None }
    }
}

/// What the viewport needs to know after the curve tool has looked at the mouse.
#[derive(Clone, Copy, Default, Debug)]
pub struct Input {
    /// The mouse is over a node handle (the gizmo stays quiet).
    pub over: bool,
    /// This press or click was used up here (no picking, no orbit, no gizmo).
    pub taken: bool,
    /// A node drag is running (no orbit).
    pub busy: bool,
}

// ---------------------------------------------------------------- the App side

impl App {
    /// The selected curve or sweep whose nodes can be edited now.
    pub(crate) fn node_shape(&self) -> Option<(usize, usize)> {
        if !self.ct.edit_nodes || self.sk.edit.is_some() {
            return None;
        }
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape(g, s).is_some_and(|x| x.kind.uses_path()) => Some((g, s)),
            _ => None,
        }
    }

    /// The chosen node of shape (g, s), if it is still there.
    pub fn chosen_node(&self, g: usize, s: usize) -> Option<usize> {
        match self.ct.node {
            Some((a, b, i)) if a == g && b == s && self.scene.shape(g, s).is_some_and(|x| i < x.path.len()) => Some(i),
            _ => None,
        }
    }

    /// The mouse for the curve tool. Call every frame before the gizmo is looked at.
    pub fn curve_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Input {
        let mut out = Input::default();
        self.check_drawing();
        if self.ct.drawing {
            return self.draw_input(ui, resp, rect, pos, &mut out);
        }
        self.ct.preview = None;
        if self.pt.on
            && let Some(used) = self.pipe_input(ui, resp, rect, pos, mods)
        {
            return used;
        }
        let Some((g, s)) = self.node_shape() else {
            self.ct.drag = None;
            self.ct.hover = None;
            return out;
        };
        let primary_down = ui.input(|i| i.pointer.primary_down());
        if self.ct.drag.is_some() {
            if primary_down && let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                self.apply_node_drag(rect, p, mods);
                out.busy = true;
                out.over = true;
                out.taken = true;
                return out;
            }
            self.ct.drag = None;
            return out;
        }
        let Some(shape) = self.scene.shape(g, s) else { return out };
        let lay = layout(&self.cam, rect, shape);
        let chosen = self.chosen_node(g, s);
        let under = pos.filter(|_| resp.hovered()).and_then(|p| hit(&lay, p, chosen));
        self.ct.hover = under;
        out.over = under.is_some();
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pos);
            if let Some(h) = origin.and_then(|o| hit(&lay, o, chosen)) {
                // Alt-drag starts from any node; plain drags need the handle itself
                if self.begin_node_drag(rect, origin, h, mods.alt) {
                    out.taken = true;
                    out.busy = true;
                    if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                        self.apply_node_drag(rect, p, mods);
                    }
                }
            }
        } else if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(h) = under
        {
            out.taken = self.click_node(h);
        }
        out
    }

    /// Something is picked up: make the drag. False when nothing could be done.
    fn begin_node_drag(&mut self, rect: Rect, origin: Option<Pos2>, h: Hit, alt: bool) -> bool {
        let Some((g, s)) = self.node_shape() else { return false };
        let cam = self.cam;
        let Some(sh) = self.scene.shape_mut(g, s) else { return false };
        let n = sh.path.len();
        let at = |sh: &Shape, i: usize| sh.path.get(i).map(|p| [p[0], p[1], p[2]]);
        let mut drag = NodeDrag { g, s, i: 0, radius: false, handle: None, level: false, grab: [0.0; 3] };
        align_handles(sh);
        match h {
            Hit::Bezier(i, outgoing) => {
                drag.i = i;
                drag.handle = Some(outgoing);
                let Some(node) = sh.path.get(i) else { return false };
                let Some(handles) = sh.curve_handles.get(i) else { return false };
                let v = if outgoing { handles.outgoing } else { handles.incoming };
                let w = to_world(sh, add([node[0],node[1],node[2]],v));
                if let Some(origin) = origin {
                    if let Some(hit) = unproject_on_plane(&cam,rect,origin,w,false) {
                        drag.grab = sub(w,hit);
                    }
                }
            }
            Hit::Radius(i) => {
                drag.i = i;
                drag.radius = true;
            }
            Hit::Node(i) if alt => {
                let Some(p) = at(sh, i) else { return false };
                let idx = if i + 1 == n {
                    extend_end(&mut sh.path, End::Last, p)
                } else if i == 0 {
                    extend_end(&mut sh.path, End::Start, p)
                } else {
                    let at4 = sh.path.get(i).copied().unwrap_or([p[0], p[1], p[2], 5.0]);
                    insert_node(&mut sh.path, i, at4)
                };
                let Some(idx) = idx else { return false };
                drag.i = idx;
            }
            Hit::Node(i) => {
                let Some(p) = at(sh, i) else { return false };
                drag.i = i;
                let w = to_world(sh, p);
                if let Some(o) = origin
                    && let Some(hit0) = unproject_on_plane(&cam, rect, o, w, false)
                {
                    drag.grab = sub(w, hit0);
                }
            }
            Hit::End(end) => {
                let i = if end == End::Start { 0 } else { n.saturating_sub(1) };
                let Some(p) = at(sh, i) else { return false };
                let Some(idx) = extend_end(&mut sh.path, end, p) else { return false };
                drag.i = idx;
            }
            Hit::Mid(seg) => {
                let (closed, smooth, _, _) = if sh.kind == Kind::Curve { curve::settings(&sh.params) } else { (false, 0.0, false, 1.0) };
                let m = curve::segment_point(&sh.path, seg, 0.5, smooth, closed && n >= 3);
                let Some(idx) = insert_node(&mut sh.path, seg, m) else { return false };
                drag.i = idx;
            }
        }
        self.ct.node = Some((g, s, drag.i));
        self.ct.drag = Some(drag);
        true
    }

    /// One frame of dragging a node (or its radius) with the pointer at `p`.
    fn apply_node_drag(&mut self, rect: Rect, p: Pos2, mods: egui::Modifiers) {
        let Some(mut d) = self.ct.drag else { return };
        let cam = self.cam;
        let Some(sh) = self.scene.shape_mut(d.g, d.s) else {
            self.ct.drag = None;
            return;
        };
        let Some(node) = sh.path.get(d.i).copied() else {
            self.ct.drag = None;
            return;
        };
        let w = to_world(sh, [node[0], node[1], node[2]]);
        if let Some(outgoing) = d.handle {
            let h = sh.curve_handles.get(d.i).copied().unwrap_or_default();
            let delta = if outgoing { h.outgoing } else { h.incoming };
            let handle_world = to_world(sh, add([node[0],node[1],node[2]],delta));
            if let Some(intersection) = unproject_on_plane(&cam,rect,p,handle_world,false) {
                let target = to_local(sh,add(intersection,d.grab));
                let mut offset = sub(target,[node[0],node[1],node[2]]);
                if mods.ctrl { offset = snap(offset, 1.0); }
                if offset.iter().all(|v| v.is_finite()) && len(offset) <= 10000.0 {
                    if let Some(h) = sh.curve_handles.get_mut(d.i) {
                        if outgoing { h.outgoing = offset; } else { h.incoming = offset; }
                        if h.mode == CurveNodeMode::Smooth {
                            if outgoing { h.incoming = offset.map(|v| -v); }
                            else { h.outgoing = offset.map(|v| -v); }
                        }
                    }
                }
            }
        } else if d.radius {
            if let Some(c) = cam.project(rect, w) {
                let thick = if sh.kind == Kind::Curve { curve::settings(&sh.params).3 } else { 1.0 };
                let k = sh.scale[0].min(sh.scale[1]).min(sh.scale[2]) * thick;
                let mut r = (p - c).length() * cam.mm_per_px(w, rect.height()) / k.max(1e-6);
                if mods.ctrl {
                    r = (r * 2.0).round() / 2.0;
                }
                if let Some(n) = sh.path.get_mut(d.i) {
                    n[3] = r.clamp(0.1, 500.0);
                }
            }
        } else if let Some(hit_now) = unproject_on_plane(&cam, rect, p, w, mods.shift) {
            if mods.shift != d.level {
                // the plane changed: keep the node where it is under the pointer
                d.level = mods.shift;
                d.grab = sub(w, hit_now);
                self.ct.drag = Some(d);
            }
            let target = add(hit_now, d.grab);
            let mut local = to_local(sh, target);
            if mods.ctrl {
                local = snap(local, 1.0);
            }
            if local.iter().all(|v| v.is_finite())
                && let Some(n) = sh.path.get_mut(d.i)
            {
                n[0] = local[0].clamp(-5000.0, 5000.0);
                n[1] = local[1].clamp(-5000.0, 5000.0);
                n[2] = local[2].clamp(-5000.0, 5000.0);
            }
        }
    }

    /// A click on a handle. Returns true when it was used.
    fn click_node(&mut self, h: Hit) -> bool {
        let Some((g, s)) = self.node_shape() else { return false };
        match h {
            Hit::Bezier(i, _) => { self.ct.node = Some((g,s,i)); true }
            Hit::Node(i) => {
                self.ct.node = Some((g, s, i));
                true
            }
            Hit::Radius(_) => true,
            Hit::End(end) => {
                let Some(sh) = self.scene.shape_mut(g, s) else { return false };
                let n = sh.path.len();
                let (a, b) = if end == End::Start { (0usize, 1usize) } else { (n.wrapping_sub(1), n.wrapping_sub(2)) };
                let (Some(pa), Some(pb)) = (sh.path.get(a).copied(), sh.path.get(b).copied()) else { return false };
                let dir = sub([pa[0], pa[1], pa[2]], [pb[0], pb[1], pb[2]]);
                let step = len(dir).max(pa[3] * 2.0).max(4.0);
                let np = add([pa[0], pa[1], pa[2]], mul(norm(dir), step));
                if let Some(idx) = extend_end(&mut sh.path, end, np) {
                    self.ct.node = Some((g, s, idx));
                }
                true
            }
            Hit::Mid(seg) => {
                let Some(sh) = self.scene.shape_mut(g, s) else { return false };
                let n = sh.path.len();
                let (closed, smooth, _, _) = if sh.kind == Kind::Curve { curve::settings(&sh.params) } else { (false, 0.0, false, 1.0) };
                let m = curve::segment_point(&sh.path, seg, 0.5, smooth, closed && n >= 3);
                if let Some(idx) = insert_node(&mut sh.path, seg, m) {
                    self.ct.node = Some((g, s, idx));
                }
                true
            }
        }
    }

    // ----- the "せん" tool -----

    /// Forget the curve being drawn if it is gone.
    fn check_drawing(&mut self) {
        if let Some((g, s)) = self.ct.draw_shape
            && !self.scene.shape(g, s).is_some_and(|x| x.kind == Kind::Curve)
        {
            self.ct.draw_shape = None;
        }
        if !self.ct.drawing {
            self.ct.draw_shape = None;
        }
    }

    /// Where a click at `p` lands: the first thing hit, else the ground (lifted by `radius`).
    pub fn scene_point(&self, rect: Rect, p: Pos2, radius: f32) -> Option<V3> {
        let (o, d) = self.cam.ray_at(rect, p);
        if let Some((t, _)) = self.field().ray(o, d, 5000.0) {
            return Some(add(o, mul(d, t)));
        }
        let g = ray_plane(o, d, [0.0; 3], [0.0, 0.0, 1.0])?;
        Some([g[0], g[1], radius])
    }

    fn draw_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, out: &mut Input) -> Input {
        self.ct.drag = None;
        self.ct.hover = None;
        let r = self.ct.draw_radius;
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < 58.0;
        self.ct.preview = match pos {
            Some(p) if resp.hovered() && !over_gnomon(p) && !ui.input(|i| i.pointer.any_down()) => self.scene_point(rect, p, r),
            _ => None,
        };
        let Some(p) = pos else { return *out };
        if !resp.hovered() || over_gnomon(p) {
            return *out;
        }
        if resp.double_clicked_by(egui::PointerButton::Primary) {
            self.finish_drawing();
            out.taken = true;
        } else if resp.clicked_by(egui::PointerButton::Primary) {
            if let Some(w) = self.scene_point(rect, p, r) {
                self.draw_node(w);
            }
            out.taken = true;
        }
        *out
    }

    /// Lay one node of the curve being drawn at the world point `w` (the first one makes the shape).
    pub fn draw_node(&mut self, w: V3) {
        let r = self.ct.draw_radius.clamp(0.1, 500.0);
        if let Some((g, s)) = self.ct.draw_shape
            && let Some(sh) = self.scene.shape_mut(g, s)
        {
            let local = to_local(sh, w);
            if extend_end(&mut sh.path, End::Last, local).is_some()
                && let Some(n) = sh.path.last_mut()
            {
                n[3] = r;
            }
            return;
        }
        if self.scene.shape_count() >= MAX_SHAPES {
            return;
        }
        let g = match self.sel {
            Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) if g < self.scene.groups.len() => g,
            _ => match self.scene.groups.len().checked_sub(1) {
                Some(g) => g,
                None => {
                    self.scene.groups.push(Group::new("グループ 1"));
                    0
                }
            },
        };
        const COLORS: [[f32; 3]; 4] = [[0.93, 0.56, 0.45], [0.60, 0.72, 0.98], [0.62, 0.90, 0.74], [0.96, 0.80, 0.45]];
        let color = COLORS[self.scene.shape_count() % COLORS.len()];
        let mut sh = Shape::new(Kind::Curve, w, Material { color, ..Material::default() });
        sh.path = vec![[0.0, 0.0, 0.0, r]];
        sh.op = self.new_op;
        let Some(group) = self.scene.groups.get_mut(g) else { return };
        group.shapes.push(sh);
        let idx = group.shapes.len() - 1;
        self.sel = Some(Pick::Shape(g, idx));
        self.ct.draw_shape = Some((g, idx));
    }

    /// Take the last node back (the whole shape when it was the only one).
    pub fn draw_undo_node(&mut self) {
        let Some((g, s)) = self.ct.draw_shape else { return };
        let Some(sh) = self.scene.shape_mut(g, s) else { return };
        sh.path.pop();
        if sh.path.is_empty()
            && let Some(group) = self.scene.groups.get_mut(g)
        {
            group.shapes.remove(s);
            self.ct.draw_shape = None;
            self.sel = Some(Pick::Group(g));
        }
    }

    /// End the curve being drawn (a lone node is no curve: it goes away). The tool stays on.
    pub fn finish_drawing(&mut self) {
        let Some((g, s)) = self.ct.draw_shape.take() else { return };
        let lone = self.scene.shape(g, s).is_some_and(|x| x.path.len() < 2);
        if lone && let Some(group) = self.scene.groups.get_mut(g) {
            group.shapes.remove(s);
            self.sel = Some(Pick::Group(g));
        }
        self.ct.node = None;
    }

    /// The top-bar toggle for the "せん" tool.
    pub fn curve_button(&mut self, ui: &mut Ui) {
        let on = self.ct.drawing;
        if style::icon_button(ui, 34.0, "せん (クリックで点をうって、線をひく)", on, glyph).clicked() {
            if on {
                self.finish_drawing();
            }
            self.ct.drawing = !on;
            self.ct.draw_shape = None;
            if self.ct.drawing {
                self.set_pipe_tool(false);
                self.sketch_off();
            }
        }
    }

    /// The little card under the top bar while the "せん" tool is on.
    pub fn draw_card(&mut self, ctx: &egui::Context) {
        if !self.ct.drawing {
            return;
        }
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("curve_card"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(pos2(rect.center().x, rect.top() + 12.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("せん").strong().color(style::ACCENT));
                        style::labeled_slider(ui, "ふとさ", &mut self.ct.draw_radius, 0.5..=40.0, "あたらしい点の太さ (半径 mm)");
                        if style::pill(ui, "おわり", false).on_hover_text("この線を終える (Enter / ダブルクリック)").clicked() {
                            self.finish_drawing();
                        }
                        if style::pill(ui, "やめる", false).on_hover_text("せんツールを切る (Esc)").clicked() {
                            self.finish_drawing();
                            self.ct.drawing = false;
                        }
                    });
                    ui.label(egui::RichText::new("クリックで点をふやす　ダブルクリック / Enterでおわり　Backspaceで1つもどる").size(11.0).color(style::DIM));
                });
            });
    }

    // ----- keys -----

    /// The keys of the curve tool. Call first in the shortcut handler; keys used here are consumed.
    pub fn curve_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let hit = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        self.pipe_keys(ctx);
        if self.ct.drawing {
            if hit(Key::Enter) {
                self.finish_drawing();
            }
            if hit(Key::Escape) {
                if self.ct.draw_shape.is_some() {
                    self.finish_drawing();
                } else {
                    self.ct.drawing = false;
                }
            }
            if hit(Key::Backspace) || hit(Key::Delete) {
                self.draw_undo_node();
            }
            return;
        }
        let Some((g, s)) = self.node_shape() else { return };
        let Some(i) = self.chosen_node(g, s) else { return };
        if hit(Key::Delete) || hit(Key::Backspace) {
            if let Some(sh) = self.scene.shape_mut(g, s)
                && delete_node(&mut sh.path, i)
            {
                self.ct.node = Some((g, s, i.saturating_sub(1)));
            }
            return;
        }
        for (k, f) in [(Key::OpenBracket, 0.9), (Key::CloseBracket, 1.1)] {
            if hit(k)
                && let Some(n) = self.scene.shape_mut(g, s).and_then(|sh| sh.path.get_mut(i))
            {
                n[3] = (n[3] * f).clamp(0.1, 500.0);
            }
        }
    }

    // ----- painting -----

    /// Draw the nodes, the `+` handles and the rubber band over the picture.
    pub fn curve_paint(&self, p: &Painter, rect: Rect) {
        let target = if self.ct.drawing { self.ct.draw_shape } else { self.node_shape() };
        let Some((g, s)) = target else {
            self.paint_preview(p, rect, None);
            return;
        };
        let Some(sh) = self.scene.shape(g, s) else { return };
        let lay = layout(&self.cam, rect, sh);
        if lay.line.len() >= 2 {
            p.add(egui::Shape::line(lay.line.clone(), Stroke::new(3.0, Color32::from_black_alpha(70))));
            p.add(egui::Shape::line(lay.line.clone(), Stroke::new(1.3, Color32::from_white_alpha(170))));
        }
        let chosen = self.chosen_node(g, s);
        let hover = if self.ct.drawing { None } else { self.ct.hover };
        if !self.ct.drawing {
            for (i, outgoing, centre, handle) in &lay.bezier {
                if chosen != Some(*i) { continue; }
                p.line_segment([*centre,*handle],Stroke::new(1.2,style::ACCENT.gamma_multiply(0.8)));
                let hot = hover == Some(Hit::Bezier(*i,*outgoing));
                p.circle_filled(*handle,if hot {6.5} else {5.0},if hot {Color32::WHITE} else {style::ACCENT});
            }
            for (i, c) in &lay.mids {
                let hot = hover == Some(Hit::Mid(*i));
                paint_plus(p, *c, MID_R * if hot { 1.25 } else { 1.0 }, hot, false);
            }
            for (end, c, from) in &lay.ends {
                let hot = hover == Some(Hit::End(*end));
                p.line_segment([*from, *c], Stroke::new(1.5, style::ACCENT.gamma_multiply(0.7)));
                paint_plus(p, *c, PLUS_R * if hot { 1.15 } else { 1.0 }, hot, true);
            }
        }
        let is_curve = sh.kind == Kind::Curve;
        for (i, n) in lay.nodes.iter().enumerate() {
            let Some(n) = n else { continue };
            let hot = hover == Some(Hit::Node(i)) || hover == Some(Hit::Radius(i)) || self.ct.drag.is_some_and(|d| d.i == i && (d.g, d.s) == (g, s));
            paint_node(p, *n, chosen == Some(i), hot, is_curve);
            if chosen == Some(i) && !self.ct.drawing && is_curve {
                let h = pos2(n.c.x + n.r, n.c.y);
                let big = hover == Some(Hit::Radius(i));
                p.circle_filled(h, if big { 6.0 } else { 4.5 }, style::ACCENT);
                p.circle_stroke(h, if big { 6.0 } else { 4.5 }, Stroke::new(1.5, Color32::WHITE));
            }
        }
        if self.ct.drawing {
            let last = lay.nodes.iter().rev().find_map(|n| *n);
            self.paint_preview(p, rect, last.map(|n| n.c));
        }
    }

    /// The rubber band from the last node to where the next one would go.
    fn paint_preview(&self, p: &Painter, rect: Rect, from: Option<Pos2>) {
        let Some(w) = self.ct.preview.filter(|_| self.ct.drawing) else { return };
        let Some(c) = self.cam.project(rect, w) else { return };
        let r = node_px(&self.cam, rect, w, self.ct.draw_radius);
        if let Some(f) = from {
            p.line_segment([f, c], Stroke::new(2.0, style::ACCENT.gamma_multiply(0.9)));
        }
        p.circle_filled(c, r, style::ACCENT.gamma_multiply(0.25));
        p.circle_stroke(c, r, Stroke::new(1.5, style::ACCENT));
    }

    // ----- inspector -----

    /// The "ふし" section of the shape panel for a curve or a sweep.
    pub fn curve_inspector(&mut self, ui: &mut Ui, g: usize, s: usize) {
        let chosen = self.chosen_node(g, s);
        let Some(sh) = self.scene.shape_mut(g, s) else { return };
        let is_curve = sh.kind == Kind::Curve;
        style::title(ui, if is_curve { "線のふし" } else { "道のふし" });
        ui.horizontal(|ui| {
            if style::pill(ui, "ふしをいじる", self.ct.edit_nodes).on_hover_text("画面で、ふしをドラッグして動かす").clicked() {
                self.ct.edit_nodes = !self.ct.edit_nodes;
            }
            ui.label(egui::RichText::new("端の＋でのばす / 間の＋でふやす").size(10.5).color(style::DIM));
        });
        if is_curve {
            align_handles(sh);
            if let Some(i) = chosen {
                if let Some(h) = sh.curve_handles.get_mut(i) {
                    ui.horizontal(|ui| {
                        ui.label("接線");
                        for (mode,label) in [(CurveNodeMode::Auto,"自動"),(CurveNodeMode::Smooth,"滑らか"),(CurveNodeMode::Corner,"角")] {
                            if ui.selectable_label(h.mode == mode,label).clicked() {
                                if h.mode == CurveNodeMode::Auto && mode != CurveNodeMode::Auto {
                                    let direction = if i>0 && i+1<sh.path.len() {
                                        sub([sh.path[i+1][0],sh.path[i+1][1],sh.path[i+1][2]],
                                            [sh.path[i-1][0],sh.path[i-1][1],sh.path[i-1][2]])
                                    } else { [3.0,0.0,0.0] };
                                    h.outgoing = mul(direction,1.0/6.0);
                                    h.incoming = mul(h.outgoing,-1.0);
                                }
                                if mode == CurveNodeMode::Smooth { h.incoming = mul(h.outgoing,-1.0); }
                                h.mode = mode;
                            }
                        }
                    });
                }
            }
            style::labeled_slider(ui, "なめらか", &mut sh.params[1], 0.0..=1.0, "0 = カクカク / 1 = なめらか");
            ui.horizontal(|ui| {
                let mut closed = sh.params[0] > 0.5;
                if ui.checkbox(&mut closed, "輪にする").on_hover_text("さいごとさいしょをつなぐ").changed() {
                    sh.params[0] = if closed { 1.0 } else { 0.0 };
                }
                let mut flat = sh.params[2] > 0.5;
                if ui.checkbox(&mut flat, "はしを平らに").on_hover_text("両はしを丸くしないで、すぱっと切る").changed() {
                    sh.params[2] = if flat { 1.0 } else { 0.0 };
                }
            });
            if !(sh.params[3].is_finite() && sh.params[3] > 0.0) {
                sh.params[3] = 1.0;
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ぜんたいの太さ").color(style::DIM).size(12.0));
                ui.add(egui::DragValue::new(&mut sh.params[3]).speed(0.01).range(0.1..=5.0).suffix(" ×").max_decimals(2));
            });
            if style::pill(ui, "ぜんぶのふしを同じ太さに", false).on_hover_text("えらんだふし(なければ先頭)の太さにそろえる").clicked() {
                let r = sh.path.get(chosen.unwrap_or(0)).map(|p| p[3]).unwrap_or(5.0);
                set_all_radii(&mut sh.path, r);
            }
        }
        let n = sh.path.len();
        let mut pick = None;
        let mut del = None;
        egui::ScrollArea::vertical().id_salt("curve_nodes").max_height(170.0).show(ui, |ui| {
            for (i, node) in sh.path.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.selectable_label(chosen == Some(i), format!("ふし {}", i + 1)).clicked() {
                        pick = Some(i);
                    }
                    ui.add(egui::DragValue::new(&mut node[3]).speed(0.1).range(0.1..=500.0).prefix(if is_curve { "太さ " } else { "倍 " }).max_decimals(1));
                    if ui.add_enabled(n > 2, egui::Button::new("×").small()).on_hover_text("このふしを消す").clicked() {
                        del = Some(i);
                    }
                });
            }
        });
        if let Some(i) = chosen
            && let Some(node) = sh.path.get_mut(i)
        {
            let mut v = [node[0], node[1], node[2]];
            if style::xyz(ui, "ふし", &mut v, 0.5, -5000.0..=5000.0, "") {
                node[0] = v[0];
                node[1] = v[1];
                node[2] = v[2];
            }
        }
        if ui.add_enabled(n < MAX_PATH, egui::Button::new("＋ ふしを足す")).on_hover_text("さいごにふやす").clicked() {
            let (a, b) = (sh.path.get(n.wrapping_sub(1)).copied(), sh.path.get(n.wrapping_sub(2)).copied());
            if let (Some(a), Some(b)) = (a, b) {
                let d = sub([a[0], a[1], a[2]], [b[0], b[1], b[2]]);
                let np = add([a[0], a[1], a[2]], mul(norm(d), len(d).max(a[3] * 2.0).max(4.0)));
                if let Some(idx) = extend_end(&mut sh.path, End::Last, np) {
                    pick = Some(idx);
                }
            }
        }
        if let Some(i) = del
            && delete_node(&mut sh.path, i)
        {
            pick = Some(i.saturating_sub(1));
        }
        if let Some(i) = pick {
            self.ct.node = Some((g, s, i));
        }
    }
}

fn paint_plus(p: &Painter, c: Pos2, r: f32, hot: bool, ring: bool) {
    p.circle_filled(c, r, if hot { style::ACCENT } else { Color32::from_rgba_unmultiplied(34, 34, 38, 225) });
    p.circle_stroke(c, r, Stroke::new(if ring { 1.8 } else { 1.2 }, if hot { Color32::WHITE } else { style::ACCENT }));
    let a = r * 0.5;
    let ink = Stroke::new(1.6, Color32::WHITE);
    p.line_segment([c - vec2(a, 0.0), c + vec2(a, 0.0)], ink);
    p.line_segment([c - vec2(0.0, a), c + vec2(0.0, a)], ink);
}

/// A node: a soft shaded ball. A curve's ball is as big as the tube is thick there.
fn paint_node(p: &Painter, n: NodeView, chosen: bool, hot: bool, big: bool) {
    let (base, ring) = if chosen {
        (style::ACCENT, Color32::WHITE)
    } else if hot {
        (Color32::from_rgb(214, 208, 255), Color32::WHITE)
    } else {
        (Color32::from_rgb(236, 236, 242), Color32::from_white_alpha(210))
    };
    if big {
        p.circle_filled(n.c, n.r, base.gamma_multiply(if chosen || hot { 0.28 } else { 0.16 }));
        p.circle_stroke(n.c, n.r, Stroke::new(if chosen { 2.2 } else { 1.4 }, ring));
    }
    let k = if big { 5.5 } else { n.r.min(8.0) };
    p.circle_filled(n.c + vec2(0.0, 1.2), k + 1.2, Color32::from_black_alpha(80));
    p.circle_filled(n.c, k, base);
    p.circle_filled(n.c + vec2(-k * 0.3, -k * 0.35), k * 0.45, Color32::from_white_alpha(170));
    p.circle_stroke(n.c, k, Stroke::new(1.2, if chosen { Color32::WHITE } else { Color32::from_black_alpha(110) }));
}

/// The icon of the "せん" tool: a wavy line with a node at each end.
pub fn glyph(p: &Painter, r: Rect, ink: Color32) {
    let pt = |x: f32, y: f32| pos2(r.min.x + r.width() * x, r.min.y + r.height() * y);
    let mut pts = Vec::new();
    for i in 0..=14 {
        let t = i as f32 / 14.0;
        pts.push(pt(0.14 + 0.72 * t, 0.5 - 0.26 * (t * std::f32::consts::TAU).sin()));
    }
    p.add(egui::Shape::line(pts, Stroke::new(r.width() * 0.09, ink)));
    for (x, y) in [(0.14, 0.5), (0.86, 0.5)] {
        p.circle_filled(pt(x, y), r.width() * 0.12, Color32::from_rgb(40, 40, 46));
        p.circle_stroke(pt(x, y), r.width() * 0.12, Stroke::new(r.width() * 0.07, ink));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 500.0))
    }

    #[test]
    fn nodes_are_inserted_extended_and_deleted() {
        let mut p = vec![[0.0, 0.0, 0.0, 4.0], [10.0, 0.0, 0.0, 6.0]];
        assert_eq!(extend_end(&mut p, End::Last, [20.0, 0.0, 0.0]), Some(2));
        assert_eq!(p[2], [20.0, 0.0, 0.0, 6.0], "takes the end's radius");
        assert_eq!(extend_end(&mut p, End::Start, [-5.0, 1.0, 0.0]), Some(0));
        assert_eq!(p[0], [-5.0, 1.0, 0.0, 4.0]);
        assert_eq!(insert_node(&mut p, 1, [2.0, 2.0, 2.0, 3.0]), Some(2));
        assert_eq!(p[2], [2.0, 2.0, 2.0, 3.0]);
        assert_eq!(p.len(), 5);
        assert!(delete_node(&mut p, 2) && p.len() == 4);
        assert!(!delete_node(&mut p, 99));
        while p.len() > 2 {
            assert!(delete_node(&mut p, 0));
        }
        assert!(!delete_node(&mut p, 0), "two nodes always stay");
        assert_eq!(insert_node(&mut p, 7, [0.0; 4]), None);
        set_all_radii(&mut p, 9.0);
        assert!(p.iter().all(|n| n[3] == 9.0));
        set_all_radii(&mut p, f32::NAN);
        assert!(p.iter().all(|n| n[3] == 5.0));
    }

    #[test]
    fn a_path_never_grows_past_the_limit() {
        let mut p = vec![[0.0, 0.0, 0.0, 4.0]; 2];
        for _ in 0..100 {
            let _ = extend_end(&mut p, End::Last, [1.0; 3]);
            let _ = insert_node(&mut p, 0, [1.0; 4]);
            let _ = extend_end(&mut p, End::Start, [1.0; 3]);
        }
        assert_eq!(p.len(), MAX_PATH);
    }

    #[test]
    fn snapping_rounds_to_the_millimetre() {
        assert_eq!(snap([1.4, -2.6, 0.49], 1.0), [1.0, -3.0, 0.0]);
    }

    #[test]
    fn world_and_local_are_inverse() {
        let mut s = Shape::new(Kind::Curve, [10.0, -5.0, 3.0], Material::default());
        s.rot = [20.0, -35.0, 70.0];
        s.scale = [1.5, 0.7, 2.0];
        for p in [[0.0, 0.0, 0.0], [12.0, -7.0, 3.0], [-30.0, 4.0, 22.0]] {
            let back = to_local(&s, to_world(&s, p));
            assert!(len(sub(back, p)) < 1e-3, "{back:?} vs {p:?}");
        }
    }

    #[test]
    fn projecting_and_unprojecting_agree() {
        for ortho in [false, true] {
            let cam = Camera { ortho, ..Camera::default() };
            let w = [14.0, -9.0, 22.0];
            let screen = cam.project(rect(), w).expect("in front");
            // a plane facing the camera through the point gives the point back
            let back = unproject_on_plane(&cam, rect(), screen, w, false).expect("hits");
            assert!(len(sub(back, w)) < 0.05, "ortho={ortho} {back:?}");
            // the level plane through the point does too
            let flat = unproject_on_plane(&cam, rect(), screen, w, true).expect("hits");
            assert!(len(sub(flat, w)) < 0.05, "ortho={ortho} {flat:?}");
            assert!((flat[2] - w[2]).abs() < 1e-3);
        }
        // a ray parallel to the plane never hits
        let mut cam = Camera::default();
        cam.look(0.0, 0.0);
        let screen = pos2(rect().center().x, rect().center().y);
        assert!(unproject_on_plane(&cam, rect(), screen, [0.0; 3], true).is_none());
    }

    #[test]
    fn the_circle_follows_the_radius() {
        let cam = Camera::default();
        let a = node_px(&cam, rect(), [0.0; 3], 4.0);
        let b = node_px(&cam, rect(), [0.0; 3], 8.0);
        assert!(b > a);
        assert!((7.0..=90.0).contains(&node_px(&cam, rect(), [0.0; 3], 1e9)));
    }

    #[test]
    fn handles_are_found_under_the_mouse() {
        let cam = Camera::default();
        let s = Shape::new(Kind::Curve, [0.0, 0.0, 10.0], Material::default());
        let l = layout(&cam, rect(), &s);
        assert_eq!(l.nodes.len(), 3);
        assert_eq!(l.mids.len(), 2);
        assert_eq!(l.ends.len(), 2);
        assert!(l.line.len() > 3);
        // the middle node
        let c = l.nodes[1].map(|n| n.c).expect("projected");
        assert_eq!(hit(&l, c, None), Some(Hit::Node(1)));
        // a node's radius dot beats the node itself
        let n0 = l.nodes[0].expect("projected");
        assert_eq!(hit(&l, pos2(n0.c.x + n0.r, n0.c.y), Some(0)), Some(Hit::Radius(0)));
        // the + beyond each end, and the + between nodes
        for (end, plus, _) in &l.ends {
            assert_eq!(hit(&l, *plus, None), Some(Hit::End(*end)));
        }
        for (i, plus) in &l.mids {
            assert_eq!(hit(&l, *plus, None), Some(Hit::Mid(*i)));
        }
        // empty space
        assert_eq!(hit(&l, pos2(15.0, 25.0), None), None);
    }

    #[test]
    fn a_closed_loop_has_no_ends_but_one_more_segment() {
        let cam = Camera::default();
        let mut s = Shape::new(Kind::Curve, [0.0, 0.0, 10.0], Material::default());
        s.params[0] = 1.0;
        let l = layout(&cam, rect(), &s);
        assert!(l.ends.is_empty());
        assert_eq!(l.mids.len(), 3);
    }

    #[test]
    fn drawing_a_curve_by_clicks_and_taking_nodes_back() {
        let mut app = App::new(true, None);
        app.ct.drawing = true;
        app.ct.draw_radius = 3.0;
        app.draw_node([10.0, 0.0, 3.0]);
        let Some((g, s)) = app.ct.draw_shape else { panic!("a shape was made") };
        assert_eq!(app.sel, Some(Pick::Shape(g, s)));
        app.draw_node([30.0, 0.0, 3.0]);
        app.ct.draw_radius = 6.0;
        app.draw_node([30.0, 20.0, 3.0]);
        let sh = app.scene.shape(g, s).expect("there");
        assert_eq!(sh.kind, Kind::Curve);
        assert_eq!(sh.pos, [10.0, 0.0, 3.0]);
        assert_eq!(sh.path.len(), 3);
        assert_eq!(sh.path[1][..3], [20.0, 0.0, 0.0]);
        assert_eq!(sh.path[0][3], 3.0);
        assert_eq!(sh.path[2][3], 6.0);
        app.draw_undo_node();
        assert_eq!(app.scene.shape(g, s).map(|x| x.path.len()), Some(2));
        app.finish_drawing();
        assert!(app.ct.draw_shape.is_none());
        assert_eq!(app.scene.shape(g, s).map(|x| x.path.len()), Some(2), "a two-node curve stays");
        // a lone node is not a curve
        let count = app.scene.shape_count();
        app.draw_node([0.0, 0.0, 3.0]);
        assert_eq!(app.scene.shape_count(), count + 1);
        app.finish_drawing();
        assert_eq!(app.scene.shape_count(), count);
        // taking back the only node removes the shape too
        app.draw_node([0.0, 0.0, 3.0]);
        app.draw_undo_node();
        assert_eq!(app.scene.shape_count(), count);
    }

    #[test]
    fn dragging_a_node_moves_it_in_the_view_plane() {
        let mut app = App::new(true, None);
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Curve);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let rect = rect();
        let w0 = {
            let sh = app.scene.shape(g, s).expect("there");
            to_world(sh, [sh.path[1][0], sh.path[1][1], sh.path[1][2]])
        };
        let c = app.cam.project(rect, w0).expect("in front");
        app.ct.drag = Some(NodeDrag { g, s, i: 1, radius: false, level: false, grab: [0.0; 3] });
        app.apply_node_drag(rect, c + vec2(40.0, 0.0), egui::Modifiers::NONE);
        let sh = app.scene.shape(g, s).expect("there");
        let w1 = to_world(sh, [sh.path[1][0], sh.path[1][1], sh.path[1][2]]);
        let (_, _, fwd) = app.cam.basis();
        assert!(dot(sub(w1, w0), fwd).abs() < 0.05, "stays in the plane facing the camera");
        assert!(len(sub(w1, w0)) > 1.0, "and moves");
        // Ctrl snaps the local numbers to whole millimetres
        app.apply_node_drag(rect, c + vec2(33.3, 12.7), egui::Modifiers::CTRL);
        let sh = app.scene.shape(g, s).expect("there");
        assert!(sh.path[1][..3].iter().all(|v| (v - v.round()).abs() < 1e-4), "{:?}", sh.path[1]);
        // Shift keeps the height
        let z = sh.path[1][2];
        app.apply_node_drag(rect, c + vec2(-30.0, 25.0), egui::Modifiers::SHIFT);
        let sh = app.scene.shape(g, s).expect("there");
        assert!((sh.path[1][2] - z).abs() < 0.05);
    }

    #[test]
    fn pulling_from_an_end_extends_the_curve() {
        let mut app = App::new(true, None);
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Curve);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("selected") };
        let before = app.scene.shape(g, s).map(|x| x.path.len()).unwrap_or(0);
        assert!(app.begin_node_drag(rect(), None, Hit::End(End::Last), false));
        assert_eq!(app.scene.shape(g, s).map(|x| x.path.len()), Some(before + 1));
        assert_eq!(app.ct.node, Some((g, s, before)));
        // Alt on a middle node inserts after it; on an end it extends
        assert!(app.begin_node_drag(rect(), None, Hit::Node(1), true));
        assert_eq!(app.scene.shape(g, s).map(|x| x.path.len()), Some(before + 2));
        assert_eq!(app.ct.node, Some((g, s, 2)));
        // the segment + inserts a node and starts moving it
        assert!(app.begin_node_drag(rect(), None, Hit::Mid(0), false));
        assert_eq!(app.ct.node, Some((g, s, 1)));
        // clicks: a node is chosen; Mid adds without a drag
        assert!(app.click_node(Hit::Node(0)));
        assert_eq!(app.ct.node, Some((g, s, 0)));
        let n = app.scene.shape(g, s).map(|x| x.path.len()).unwrap_or(0);
        assert!(app.click_node(Hit::End(End::Start)));
        assert_eq!(app.scene.shape(g, s).map(|x| x.path.len()), Some(n + 1));
    }
}
