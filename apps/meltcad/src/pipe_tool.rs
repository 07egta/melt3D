//! The "パイプ" tool: press and drag across the viewport to trace a line; on release it becomes ONE
//! `Kind::Curve` shape, a smooth solid connector (or an optional hollow pipe) that follows the stroke.
//!
//! The maths (projection onto the model's skin or a plane, simplifying, smoothing) is in
//! `stroke.rs`; this file is the glue to the mouse, the keys, the little card and the icon. Editing
//! the pipe afterwards is the ordinary curve editing of `curve_tool.rs` (nodes, radii, smoothness).

use egui::{Color32, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::curve_tool::{self, Input};
use crate::model::{Kind, MAX_SHAPES, Material, Pick, Shape};
use crate::sdf::V3;
use crate::stroke::{self, MAX_STROKE, PIPE_NODES, Plane};
use crate::style;

/// Pixels between the samples taken along a fast mouse move.
const SAMPLE_PX: f32 = 5.0;
/// Most extra samples one frame's mouse move is cut into.
const MAX_SPLIT: usize = 24;

/// What the next pipe (and the last one, live) is made of.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Config {
    /// Outer radius (mm).
    pub radius: f32,
    /// Wall thickness (mm). 0 = a solid rod.
    pub wall: f32,
    /// A closed loop.
    pub closed: bool,
    /// Open (cut straight, the bore shows) instead of round ends.
    pub open_ends: bool,
    pub color: [f32; 3],
}

impl Default for Config {
    fn default() -> Self {
        Config { radius: 4.0, wall: 0.0, closed: false, open_ends: false, color: [0.76, 0.80, 0.88] }
    }
}

impl Config {
    /// Keep every number in a range the shape maths is happy with.
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        fix(&mut self.radius, 0.3, 100.0, 4.0);
        // a wall as thick as the radius would leave no bore: that is a rod
        let max_wall = (self.radius - 0.1).max(0.0);
        fix(&mut self.wall, 0.0, max_wall, 0.0);
        for c in &mut self.color {
            fix(c, 0.0, 1.0, 0.8);
        }
    }
}

pub struct State {
    /// The tool is on: dragging in the viewport traces a pipe.
    pub on: bool,
    pub cfg: Config,
    /// Where a stroke lands when it misses the model.
    pub plane: Plane,
    /// The stroke being traced (world points); empty when not tracing.
    pub stroke: Vec<V3>,
    /// Whether each accepted sample hit the model rather than the working plane.
    pub stroke_skin: Vec<bool>,
    /// Plane positions of accepted samples, used to bound a bridge between model hits.
    pub stroke_plane: Vec<V3>,
    pub tracing: bool,
    /// The last cursor position that was sampled (the next move is filled in from here).
    pub last_screen: Option<Pos2>,
    /// Where a stroke would start under the cursor (for the little ring).
    pub hover: Option<V3>,
    /// The pipe made last (group, shape): the card edits it while it is selected.
    pub last: Option<(usize, usize)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            on: false,
            cfg: Config::default(),
            plane: Plane::Floor,
            stroke: Vec::new(),
            stroke_skin: Vec::new(),
            stroke_plane: Vec::new(),
            tracing: false,
            last_screen: None,
            hover: None,
            last: None,
        }
    }
}

/// Put the card's choices on a pipe. `only` limits it to what just changed (so editing the colour
/// does not undo a radius the person set node by node).
fn restyle(sh: &mut Shape, now: &Config, before: Option<&Config>) {
    let changed = |f: fn(&Config) -> bool| before.is_none_or(|b| f(b) != f(now));
    if before.is_none_or(|b| b.radius != now.radius) {
        curve_tool::set_all_radii(&mut sh.path, now.radius);
    }
    if before.is_none_or(|b| b.wall != now.wall) {
        sh.shell = now.wall;
    }
    if changed(|c| c.closed) {
        sh.params[0] = if now.closed { 1.0 } else { 0.0 };
    }
    if changed(|c| c.open_ends) {
        sh.params[2] = if now.open_ends { 1.0 } else { 0.0 };
    }
    if before.is_none_or(|b| b.color != now.color) {
        sh.material.color = now.color;
    }
}

impl App {
    /// Switch the pipe tool on or off. It excludes the click-to-place "せん" tool.
    pub fn set_pipe_tool(&mut self, on: bool) {
        if on {
            self.finish_drawing();
            self.ct.drawing = false;
            self.sketch_off();
        }
        self.pt.on = on;
        self.pt.stroke.clear();
        self.pt.stroke_skin.clear();
        self.pt.stroke_plane.clear();
        self.pt.tracing = false;
        self.pt.last_screen = None;
        self.pt.hover = None;
    }

    /// The top-bar toggle for the pipe tool.
    pub fn pipe_button(&mut self, ui: &mut Ui) {
        let on = self.pt.on;
        if style::wide_button(ui, "Pipe", "Pipe (drag across the view to trace a pipe)", on, None, glyph).clicked() {
            self.set_pipe_tool(!on);
        }
    }

    /// Where the cursor at `p` would put a stroke point.
    fn pipe_point(&self, rect: Rect, p: Pos2) -> Option<V3> {
        let (o, d) = self.cam.ray_at(rect, p);
        stroke::trace_point(&self.field(), o, d, self.pt.plane, self.pt.cfg.radius)
    }

    /// Add the stroke points for a cursor move to `p` (long moves are filled in every few pixels).
    pub fn pipe_sample(&mut self, rect: Rect, p: Pos2) {
        let from = self.pt.last_screen.unwrap_or(p);
        let dist = (p - from).length();
        let n = if dist.is_finite() { ((dist / SAMPLE_PX).ceil() as usize).clamp(1, MAX_SPLIT) } else { 1 };
        let field = self.field();
        for k in 1..=n {
            if self.pt.stroke.len() >= MAX_STROKE {
                break;
            }
            let q = from + (p - from) * (k as f32 / n as f32);
            let (o, d) = self.cam.ray_at(rect, q);
            let skin = stroke::surface_point(&field, o, d, self.pt.cfg.radius);
            let plane = stroke::plane_point(o, d, self.pt.plane, self.pt.cfg.radius);
            if let Some(w) = skin.or(plane) {
                self.pt.stroke.push(w);
                self.pt.stroke_skin.push(skin.is_some());
                self.pt.stroke_plane.push(plane.unwrap_or(w));
            }
        }
        self.pt.last_screen = Some(p);
    }

    /// End the stroke: make the pipe (nothing for a click). True when a pipe was made.
    pub fn pipe_finish(&mut self) -> bool {
        self.pt.tracing = false;
        self.pt.last_screen = None;
        let mut raw = std::mem::take(&mut self.pt.stroke);
        stroke::bridge_surface_gaps(&mut raw, &self.pt.stroke_skin, &self.pt.stroke_plane, self.pt.cfg.radius);
        self.pt.stroke_skin.clear();
        self.pt.stroke_plane.clear();
        self.pipe_from_stroke(&raw)
    }

    /// Throw the stroke away (Esc).
    pub fn pipe_cancel(&mut self) {
        self.pt.tracing = false;
        self.pt.last_screen = None;
        self.pt.stroke.clear();
        self.pt.stroke_skin.clear();
        self.pt.stroke_plane.clear();
    }

    /// The tolerance and minimum length for a stroke at this zoom and radius.
    fn pipe_options(&self, raw: &[V3]) -> stroke::Options {
        let cfg = &self.pt.cfg;
        let mm_px = self.cam.mm_per_px(stroke::centre(raw), self.view_rect.height()).clamp(0.001, 100.0);
        stroke::Options {
            tol: (mm_px * 3.0).max(cfg.radius * 0.15).max(0.2),
            max_nodes: PIPE_NODES,
            closed: cfg.closed,
            min_len: cfg.radius.max(2.0),
        }
    }

    /// Make a `Kind::Curve` pipe from raw world points. False when the stroke is no line, or the
    /// scene is full.
    pub fn pipe_from_stroke(&mut self, raw: &[V3]) -> bool {
        if self.scene.shape_count() >= MAX_SHAPES {
            return false;
        }
        let Some(nodes) = stroke::pipe_nodes(raw, &self.pipe_options(raw)) else { return false };
        let Some(g) = self.target_group() else { return false };
        let mut cfg = self.pt.cfg;
        cfg.sanitize();
        let pos = stroke::centre(&nodes);
        let mut sh = Shape::new(Kind::Curve, pos, Material::plain(cfg.color, 0.4, 0.0));
        sh.name = "Pipe".to_string();
        sh.path = nodes.iter().map(|p| [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2], cfg.radius]).collect();
        sh.params = [0.0, 1.0, 0.0, 1.0];
        sh.blend = 2.0;
        sh.op = self.new_op;
        restyle(&mut sh, &cfg, None);
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        group.shapes.push(sh);
        let idx = group.shapes.len() - 1;
        self.sel = Some(Pick::Shape(g, idx));
        self.pt.last = Some((g, idx));
        self.ct.node = None;
        true
    }

    /// The card's choices changed: put them on the pipe made last, if it is still the selected one.
    fn pipe_restyle_last(&mut self, before: &Config) {
        let Some((g, s)) = self.pt.last else { return };
        if self.sel != Some(Pick::Shape(g, s)) {
            return;
        }
        let now = self.pt.cfg;
        if let Some(sh) = self.scene.shape_mut(g, s)
            && sh.kind == Kind::Curve
        {
            restyle(sh, &now, Some(before));
        }
    }

    /// Whether the pipe made last is the selected shape (so the card has something to edit).
    fn pipe_last_selected(&self) -> bool {
        match (self.pt.last, self.sel) {
            (Some((g, s)), Some(Pick::Shape(sg, ss))) => g == sg && s == ss && self.scene.shape(g, s).is_some_and(|x| x.kind == Kind::Curve),
            _ => false,
        }
    }

    /// The mouse while the pipe tool is on. Some = this frame's mouse was used up here. None = not
    /// ours (a press on a node handle of the selected pipe, on the view cube, with Alt): the curve
    /// node editing carries on as usual.
    pub fn pipe_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Option<Input> {
        let taken = Input { over: true, taken: true, busy: true };
        let primary_down = ui.input(|i| i.pointer.primary_down());
        if self.pt.tracing {
            if primary_down {
                if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                    self.pipe_sample(rect, p);
                }
                return Some(taken);
            }
            // one drag, one pipe: the tool ends by itself and the new pipe stays selected, so the
            // card keeps fixing it (radius, wall, ends). Press Pipe again for another.
            if self.pipe_finish() {
                self.pt.on = false;
                self.pt.hover = None;
            }
            return Some(Input { over: true, taken: true, busy: false });
        }
        // a right click ends the tool, like Esc
        if resp.clicked_by(egui::PointerButton::Secondary) {
            self.set_pipe_tool(false);
            return Some(Input { over: true, taken: true, busy: false });
        }
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < 58.0;
        self.pt.hover = match pos {
            Some(p) if resp.hovered() && !over_gnomon(p) && !primary_down && !mods.alt => self.pipe_point(rect, p),
            _ => None,
        };
        if resp.drag_started_by(egui::PointerButton::Primary) && !mods.alt {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pos)?;
            if over_gnomon(origin) {
                return None;
            }
            // a press on a handle of the selected pipe edits the pipe instead of tracing
            if let Some((g, s)) = self.node_shape()
                && let Some(shape) = self.scene.shape(g, s)
            {
                let lay = curve_tool::layout(&self.cam, rect, shape);
                if curve_tool::hit(&lay, origin, self.chosen_node(g, s)).is_some() {
                    return None;
                }
            }
            self.pt.stroke.clear();
            self.pt.stroke_skin.clear();
            self.pt.stroke_plane.clear();
            self.pt.last_screen = None;
            self.pt.tracing = true;
            self.pt.hover = None;
            self.pipe_sample(rect, origin);
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                self.pipe_sample(rect, p);
            }
            return Some(taken);
        }
        None
    }

    // ----- keys -----

    /// Esc: drop the stroke being traced, or else switch the tool off. Other keys stay with the node editing.
    pub fn pipe_keys(&mut self, ctx: &egui::Context) {
        if !self.pt.on {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.pt.tracing {
                self.pipe_cancel();
            } else {
                self.set_pipe_tool(false);
            }
        }
    }

    // ----- painting -----

    /// The stroke being traced and the ring that shows the pipe's size under the cursor.
    pub fn pipe_paint(&self, p: &Painter, rect: Rect) {
        if !self.pt.on {
            return;
        }
        if self.pt.stroke.len() >= 2 {
            let pts: Vec<Pos2> = self.pt.stroke.iter().filter_map(|w| self.cam.project(rect, *w)).collect();
            if pts.len() >= 2 {
                let mm_px = self.cam.mm_per_px(stroke::centre(&self.pt.stroke), rect.height()).max(1e-4);
                let width = (self.pt.cfg.radius * 2.0 / mm_px).clamp(3.0, 80.0);
                p.add(egui::Shape::line(pts.clone(), Stroke::new(width, style::ACCENT.gamma_multiply(0.25))));
                p.add(egui::Shape::line(pts, Stroke::new(1.6, style::ACCENT)));
            }
        }
        if let Some(w) = self.pt.hover.filter(|_| !self.pt.tracing)
            && let Some(c) = self.cam.project(rect, w)
        {
            let r = curve_tool::node_px(&self.cam, rect, w, self.pt.cfg.radius);
            p.circle_filled(c, r, style::ACCENT.gamma_multiply(0.22));
            p.circle_stroke(c, r, Stroke::new(1.5, style::ACCENT));
        }
    }

    // ----- the card -----

    /// The little card under the top bar while the tool is on (and while its pipe is selected).
    pub fn pipe_card(&mut self, ctx: &egui::Context) {
        if !(self.pt.on || self.pipe_last_selected()) {
            return;
        }
        let rect = self.view_rect;
        let before = self.pt.cfg;
        let mut off = false;
        egui::Area::new(egui::Id::new("pipe_card"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(pos2(rect.center().x, rect.top() + 12.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    let cfg = &mut self.pt.cfg;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Pipe").strong().color(style::ACCENT));
                        if self.pt.on {
                            ui.label(egui::RichText::new("Drag once to draw a pipe").size(11.0).color(style::DIM));
                            if style::pill(ui, "Stop", false).on_hover_text("Stop drawing pipes (Esc or right click)").clicked() {
                                off = true;
                            }
                        } else {
                            ui.label(egui::RichText::new("Fix the selected pipe").size(11.0).color(style::DIM));
                        }
                    });
                    style::labeled_slider(ui, "Outer radius", &mut cfg.radius, 0.5..=40.0, "Outer radius of the pipe (mm)");
                    let max_wall = (cfg.radius - 0.1).clamp(0.0, 20.0);
                    style::labeled_slider(ui, "Wall\nthickness", &mut cfg.wall, 0.0..=max_wall.max(0.1), "0 = solid rod / larger = hollow pipe (mm)");
                    ui.horizontal(|ui| {
                        if style::pill(ui, "Closed loop", cfg.closed).on_hover_text("Join the last and first node").clicked() {
                            cfg.closed = !cfg.closed;
                        }
                        ui.label(egui::RichText::new("Ends").size(11.0).color(style::DIM));
                        if style::pill(ui, "Round", !cfg.open_ends).on_hover_text("Close the ends round").clicked() {
                            cfg.open_ends = false;
                        }
                        if style::pill(ui, "Flat", cfg.open_ends).on_hover_text("Cut the ends straight (shows the hole)").clicked() {
                            cfg.open_ends = true;
                        }
                        style::color_edit(ui, &mut cfg.color);
                    });
                    if self.pt.on {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Trace plane").size(11.0).color(style::DIM));
                            for pl in Plane::ALL {
                                if style::pill(ui, pl.label(), self.pt.plane == pl)
                                    .on_hover_text("Where there is no shape, trace on this plane")
                                    .clicked()
                                {
                                    self.pt.plane = pl;
                                }
                            }
                        });
                        ui.label(
                            egui::RichText::new("Over a shape it follows the surface · middle drag turns the view").size(11.0).color(style::DIM),
                        );
                    }
                });
            });
        self.pt.cfg.sanitize();
        if self.pt.cfg != before {
            self.pipe_restyle_last(&before);
        }
        if off {
            self.set_pipe_tool(false);
        }
    }
}

/// The icon of the pipe tool: a thin outline of a bent pipe with its open end showing the hole.
pub fn glyph(p: &Painter, r: Rect, ink: Color32) {
    // What the tool does: drag across the view and a fat, round tube follows the mouse. So: a thick
    // round stroke (a tube, lit on top) drawn freehand, with the pencil still drawing it.
    let side = r.width().min(r.height());
    let f = Rect::from_center_size(r.center(), vec2(side, side));
    let at = |u: f32, v: f32| pos2(f.left() + f.width() * u, f.top() + f.height() * v);
    let path: Vec<Pos2> = (0..=20)
        .map(|i| {
            let t = i as f32 / 20.0;
            at(0.10 + 0.56 * t, 0.70 - 0.30 * t + 0.12 * (t * std::f32::consts::TAU).sin())
        })
        .collect();
    let fat = side * 0.22;
    p.add(egui::Shape::line(path.clone(), Stroke::new(fat, ink.gamma_multiply(0.55))));
    for e in [path[0], path[path.len() - 1]] {
        p.circle_filled(e, fat * 0.5, ink.gamma_multiply(0.55));
    }
    // the shine along the top: it reads as round, a tube, not a flat line
    let shine: Vec<Pos2> = path.iter().map(|q| *q + vec2(-fat * 0.12, -fat * 0.2)).collect();
    p.add(egui::Shape::line(shine, Stroke::new(fat * 0.28, ink)));
    // the pencil at the end of the tube, still drawing it
    let tip = path[path.len() - 1] + vec2(fat * 0.35, -fat * 0.35);
    let d = vec2(0.7071, -0.7071);
    let n = vec2(0.7071, 0.7071);
    let w = side * 0.085;
    let l = side * 0.34;
    let body = vec![tip + d * (w * 1.3) + n * w, tip + d * l + n * w, tip + d * l - n * w, tip + d * (w * 1.3) - n * w];
    p.add(egui::Shape::convex_polygon(body, ink, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![tip, tip + d * (w * 1.3) + n * w, tip + d * (w * 1.3) - n * w], ink.gamma_multiply(0.7), Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::sdf::{len, sub};

    fn rect() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 500.0))
    }

    fn app_with_view() -> App {
        let mut app = App::new(true, None);
        app.view_rect = rect();
        app
    }

    #[test]
    fn a_traced_stroke_becomes_one_solid_pipe() {
        let mut app = app_with_view();
        let before = app.scene.shape_count();
        let raw: Vec<V3> = (0..120).map(|i| [i as f32, 20.0 * (i as f32 * 0.08).sin(), 4.0]).collect();
        assert!(app.pipe_from_stroke(&raw));
        assert_eq!(app.scene.shape_count(), before + 1, "exactly one shape");
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("the pipe is selected") };
        assert_eq!(app.pt.last, Some((g, s)));
        let sh = app.scene.shape(g, s).expect("there");
        assert_eq!(sh.kind, Kind::Curve);
        assert!(sh.path.len() >= 3 && sh.path.len() <= PIPE_NODES, "{} nodes", sh.path.len());
        assert!(sh.path.iter().all(|n| n[3] == 4.0), "radius 4");
        assert_eq!(sh.shell, 0.0, "solid by default");
        assert_eq!(sh.params[2], 0.0, "round ends by default");
        assert_eq!(sh.params[0], 0.0);
        // the nodes lie where the stroke was
        let w0 = curve_tool::to_world(sh, [sh.path[0][0], sh.path[0][1], sh.path[0][2]]);
        assert!(len(sub(w0, raw[0])) < 1e-3, "the start node is the start of the stroke");
        // the shape is valid after a round trip through the file format
        let json = app.scene.to_json();
        let back = crate::model::Scene::from_json(&json).expect("loads");
        assert_eq!(back.shape(g, s).map(|x| x.path.len()), Some(sh.path.len()));
    }

    #[test]
    fn a_new_pipe_has_a_solid_core_and_round_ends() {
        let mut app = app_with_view();
        let raw: Vec<V3> = (0..60).map(|i| [i as f32, 0.0, 4.0]).collect();
        assert!(app.pipe_from_stroke(&raw));
        let Some((g, s)) = app.pt.last else { panic!("made") };
        let pipe = app.scene.shape(g, s).expect("pipe").clone();
        let field = crate::sdf::Field::new(&crate::model::Scene {
            groups: vec![crate::model::Group { shapes: vec![pipe], ..crate::model::Group::new("g") }],
            melt: Default::default(),
            env: Default::default(),
            saved: Vec::new(),
            anim: Default::default(),
        });
        assert!(field.distance([30.0, 0.0, 4.0]) < -3.9, "the centre of the connector is solid");
        assert!(field.distance([-2.0, 0.0, 4.0]) < -1.9, "the first end is rounded");
        assert!(field.distance([61.0, 0.0, 4.0]) < -1.9, "the last end is rounded");
    }

    #[test]
    fn clicks_and_empty_strokes_make_nothing() {
        let mut app = app_with_view();
        let n = app.scene.shape_count();
        assert!(!app.pipe_from_stroke(&[]));
        assert!(!app.pipe_from_stroke(&[[0.0; 3]]));
        assert!(!app.pipe_from_stroke(&[[3.0, 3.0, 3.0]; 30]));
        // a cursor that never moved: finishing a stroke with one sample makes nothing
        app.pt.on = true;
        app.pt.tracing = true;
        app.pipe_sample(rect(), pos2(300.0, 300.0));
        assert!(!app.pipe_finish());
        assert!(!app.pt.tracing && app.pt.stroke.is_empty());
        assert_eq!(app.scene.shape_count(), n);
    }

    #[test]
    fn a_drag_over_the_floor_traces_a_line_lifted_by_the_radius() {
        let mut app = app_with_view();
        // look straight down so every cursor ray meets the floor (the default scene's ball is far away)
        app.cam = Camera::default();
        app.cam.look(0.0, std::f32::consts::FRAC_PI_2 - 0.05);
        app.cam.target = [200.0, 200.0, 0.0];
        app.cam.dist = 120.0;
        app.pt.cfg.radius = 3.0;
        app.pt.on = true;
        app.pt.tracing = true;
        let r = rect();
        let c = r.center();
        for k in 0..30 {
            app.pipe_sample(r, c + vec2(k as f32 * 6.0 - 80.0, (k as f32 * 0.3).sin() * 40.0));
        }
        assert!(app.pt.stroke.len() >= 30, "{} samples", app.pt.stroke.len());
        assert!(app.pt.stroke.iter().all(|p| (p[2] - 3.0).abs() < 1e-3), "on the floor, one radius up");
        // a long jump is filled in
        let before = app.pt.stroke.len();
        app.pipe_sample(r, c + vec2(300.0, 100.0));
        assert!(app.pt.stroke.len() > before + 5);
        let n = app.scene.shape_count();
        assert!(app.pipe_finish());
        assert_eq!(app.scene.shape_count(), n + 1);
    }

    #[test]
    fn a_drag_over_the_model_hugs_its_skin() {
        let mut app = app_with_view();
        // the starting ball: radius 20 (default size) at its position
        let (c, r0) = {
            let s = app.scene.shape(0, 0).expect("a ball");
            (s.pos, s.size[0])
        };
        app.cam = Camera::default();
        app.cam.target = c;
        app.cam.dist = 160.0;
        app.pt.cfg.radius = 2.5;
        app.pt.on = true;
        app.pt.tracing = true;
        let r = rect();
        let mid = r.center();
        for k in 0..20 {
            app.pipe_sample(r, mid + vec2(k as f32 * 2.0 - 20.0, (k as f32 * 0.4).sin() * 8.0));
        }
        assert!(!app.pt.stroke.is_empty());
        for p in &app.pt.stroke {
            let d = len(sub(*p, c));
            assert!((d - (r0 + 2.5)).abs() < 0.3, "a sample sits one pipe radius off the skin: {d} vs {}", r0 + 2.5);
        }
    }

    #[test]
    fn the_card_edits_the_last_pipe_only_while_it_is_selected() {
        let mut app = app_with_view();
        let raw: Vec<V3> = (0..60).map(|i| [i as f32, 0.0, 4.0]).collect();
        assert!(app.pipe_from_stroke(&raw));
        let Some((g, s)) = app.pt.last else { panic!("made") };
        let before = app.pt.cfg;
        app.pt.cfg.radius = 6.0;
        app.pt.cfg.wall = 0.0;
        app.pt.cfg.closed = true;
        app.pt.cfg.open_ends = false;
        app.pt.cfg.color = [1.0, 0.0, 0.0];
        app.pipe_restyle_last(&before);
        let sh = app.scene.shape(g, s).expect("there");
        assert!(sh.path.iter().all(|n| n[3] == 6.0));
        assert_eq!((sh.shell, sh.params[0], sh.params[2], sh.material.color), (0.0, 1.0, 0.0, [1.0, 0.0, 0.0]));
        // with something else selected, the pipe is left alone
        app.sel = Some(Pick::Shape(0, 0));
        let before = app.pt.cfg;
        app.pt.cfg.radius = 9.0;
        app.pipe_restyle_last(&before);
        assert!(app.scene.shape(g, s).is_some_and(|x| x.path.iter().all(|n| n[3] == 6.0)));
        // a change of colour does not undo a node radius set by hand
        app.sel = Some(Pick::Shape(g, s));
        if let Some(n) = app.scene.shape_mut(g, s).and_then(|x| x.path.get_mut(1)) {
            n[3] = 11.0;
        }
        let before = app.pt.cfg;
        app.pt.cfg.color = [0.0, 1.0, 0.0];
        app.pipe_restyle_last(&before);
        assert_eq!(app.scene.shape(g, s).map(|x| x.path[1][3]), Some(11.0));
    }

    #[test]
    fn the_config_stays_sane() {
        let mut c = Config { radius: f32::NAN, wall: 99.0, closed: false, open_ends: true, color: [2.0, f32::NAN, -1.0] };
        c.sanitize();
        assert_eq!(c.radius, 4.0);
        assert!(c.wall <= c.radius - 0.1 + 1e-6);
        assert!(c.color.iter().all(|v| (0.0..=1.0).contains(v)));
        let mut thin = Config { radius: 0.3, wall: 5.0, ..Config::default() };
        thin.sanitize();
        assert!(thin.wall < thin.radius);
    }

    #[test]
    fn the_tools_exclude_each_other_and_esc_ends_the_pipe_tool() {
        let ctx = egui::Context::default();
        let mut app = app_with_view();
        app.ct.drawing = true;
        app.set_pipe_tool(true);
        assert!(app.pt.on && !app.ct.drawing);
        app.pt.tracing = true;
        app.pt.stroke.push([0.0; 3]);
        app.pipe_cancel();
        assert!(!app.pt.tracing && app.pt.stroke.is_empty() && app.pt.on);
        // Esc with the tool on and nothing traced switches it off
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        app.pipe_keys(&ctx);
        assert!(!app.pt.on);
    }

    #[test]
    fn the_card_and_the_icon_draw_without_a_graphics_card() {
        let ctx = egui::Context::default();
        crate::fonts::setup(&ctx);
        style::apply(&ctx);
        let mut app = app_with_view();
        app.set_pipe_tool(true);
        let raw: Vec<V3> = (0..60).map(|i| [i as f32, (i as f32 * 0.2).sin() * 10.0, 4.0]).collect();
        assert!(app.pipe_from_stroke(&raw));
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1380.0, 860.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                app.pipe_card(ui.ctx());
                let mut stripped = false;
                egui::Area::new(egui::Id::new("t")).show(ui.ctx(), |ui| {
                    app.pipe_button(ui);
                    stripped = true;
                });
                assert!(stripped);
            });
            assert!(!out.shapes.is_empty());
            // This headless test has no painter to consume texture uploads.
            out.textures_delta.clear();
        }
        // with the tool off and the pipe not selected there is no card
        app.set_pipe_tool(false);
        app.sel = Some(Pick::Shape(0, 0));
        assert!(!app.pipe_last_selected());
    }

    #[test]
    fn a_hollow_pipe_is_a_wall_around_an_empty_bore_open_at_both_ends() {
        let mut app = app_with_view();
        let raw: Vec<V3> = (0..60).map(|i| [i as f32, 0.0, 4.0]).collect();
        app.pt.cfg = Config { radius: 4.0, wall: 1.2, closed: false, open_ends: true, color: [0.8; 3] };
        assert!(app.pipe_from_stroke(&raw));
        let Some((g, s)) = app.pt.last else { panic!("made") };
        let sh = app.scene.shape(g, s).expect("there").clone();
        let field = crate::sdf::Field::new(&crate::model::Scene {
            groups: vec![crate::model::Group { shapes: vec![sh], ..crate::model::Group::new("g") }],
            melt: Default::default(),
            env: Default::default(),
            saved: Vec::new(),
            anim: Default::default(),
        });
        // the pipe lies along x from 0 to 59 at y = 0, z = 4
        let d = |x: f32, y: f32, z: f32| field.distance([x, y, z]);
        assert!(d(30.0, 3.4, 4.0) < 0.0, "inside the wall");
        assert!(d(30.0, 0.0, 4.0) > 1.0, "the bore is empty");
        assert!(d(30.0, 6.0, 4.0) > 1.9, "outside");
        // open ends: the bore can be seen into from the end (no cap), and nothing sticks out past it
        assert!(d(-1.0, 0.0, 4.0) > 0.9, "no cap over the bore");
        assert!(d(-0.5, 3.4, 4.0) > 0.0, "the wall stops at the end plane");
        assert!(d(0.5, 3.4, 4.0) < 0.0, "but is there just inside it");
        assert!(d(60.0, 0.0, 4.0) > 0.9);
        // a solid rod has no bore
        let before = app.pt.cfg;
        app.pt.cfg.wall = 0.0;
        app.pipe_restyle_last(&before);
        assert_eq!(app.scene.shape(g, s).map(|x| x.shell), Some(0.0));
    }

    /// `YT_OUT=/tmp/pipes.yura cargo test -p meltcad dump_pipe_scene -- --ignored` writes a scene with
    /// pipes traced over a ball and over the floor, to look at with `meltcad --render`.
    #[test]
    #[ignore]
    fn dump_pipe_scene() {
        let Ok(out) = std::env::var("YT_OUT") else { return };
        let mut app = app_with_view();
        // a spiral over the ball (radius 20 at the default position), traced as the cursor would
        app.cam = Camera::default();
        let c = app.scene.shape(0, 0).map(|s| s.pos).unwrap_or([0.0; 3]);
        app.cam.target = c;
        app.cam.dist = 120.0;
        app.pt.cfg.radius = 2.0;
        app.pt.cfg.wall = 0.8;
        app.pt.tracing = true;
        let r = rect();
        for k in 0..60 {
            let t = k as f32 / 59.0;
            app.pipe_sample(r, r.center() + vec2(-60.0 + 120.0 * t, 35.0 * (t * 9.0).sin()));
        }
        app.pipe_finish();
        // a loop on the floor
        app.pt.cfg = Config { radius: 5.0, wall: 1.5, closed: true, open_ends: false, color: [0.9, 0.5, 0.3] };
        let ring: Vec<V3> = (0..=100)
            .map(|i| {
                let a = i as f32 / 100.0 * std::f32::consts::TAU;
                [c[0] + 70.0 + 30.0 * a.cos(), c[1] + 30.0 * a.sin(), 5.0]
            })
            .collect();
        app.pipe_from_stroke(&ring);
        // an open-ended elbow, so the bore shows
        app.pt.cfg = Config { radius: 6.0, wall: 1.5, closed: false, open_ends: true, color: [0.6, 0.75, 0.95] };
        let elbow: Vec<V3> = (0..=40)
            .map(|i| {
                let t = i as f32 / 40.0;
                if t < 0.5 { [c[0] - 70.0 + t * 80.0, c[1] - 50.0, 6.0] } else { [c[0] - 70.0 + 40.0, c[1] - 50.0, 6.0 + (t - 0.5) * 80.0] }
            })
            .collect();
        app.pipe_from_stroke(&elbow);
        let _ = std::fs::write(out, app.scene.to_json());
    }
}
