//! The viewport: camera controls, picking and the handles' input.

use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::camera::Camera;
use crate::gizmo;
use crate::model::{Kind, Pick, Scene};
use crate::perf::Gate;
use crate::sdf::{V3, add, dot, len, mul, sub};
use crate::style;

/// The home button at the upper left of the axis ball.
const HOME_RADIUS: f32 = 12.0;

/// The pointing device the view controls are tuned for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Device {
    Mouse,
    Trackpad,
}

/// The person's choice: a fixed device, or follow what the scrolling looks like.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputDev {
    Auto,
    Mouse,
    Trackpad,
}

impl InputDev {
    /// The device in use: the fixed choice, or what was last seen.
    pub fn device(self, seen: Device) -> Device {
        match self {
            InputDev::Auto => seen,
            InputDev::Mouse => Device::Mouse,
            InputDev::Trackpad => Device::Trackpad,
        }
    }
}

/// What a scroll event says about the device: a notched wheel sends whole lines; a trackpad sends
/// pixels or fractions of a line (and pinches).
pub fn device_of_scroll(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> Option<Device> {
    if !delta.x.is_finite() || !delta.y.is_finite() || (delta.x == 0.0 && delta.y == 0.0) {
        return None;
    }
    match unit {
        egui::MouseWheelUnit::Point => Some(Device::Trackpad),
        egui::MouseWheelUnit::Page => Some(Device::Mouse),
        egui::MouseWheelUnit::Line => {
            let whole = |v: f32| (v - v.round()).abs() < 1e-3;
            if whole(delta.x) && whole(delta.y) { Some(Device::Mouse) } else { Some(Device::Trackpad) }
        }
    }
}

/// The shapes a pick stands for.
pub fn targets(scene: &Scene, pick: Option<Pick>) -> Vec<(usize, usize)> {
    match pick {
        Some(Pick::Shape(g, s)) if scene.shape(g, s).is_some() => vec![(g, s)],
        Some(Pick::Group(g)) => scene.groups.get(g).map(|gr| (0..gr.shapes.len()).map(|s| (g, s)).collect()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The shapes of all these picks, each once.
pub fn targets_all(scene: &Scene, picks: &[Pick]) -> Vec<(usize, usize)> {
    if picks.len() <= 1 {
        return targets(scene, picks.first().copied());
    }
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(picks.len());
    let mut seen = std::collections::HashSet::with_capacity(picks.len());
    for p in picks {
        // Visit each selected range directly: no temporary Vec for each pick, and
        // no growing linear scan of all earlier selections for every shape.
        let (g, range) = match *p {
            Pick::Shape(g, s) if scene.shape(g, s).is_some() => (g, s..s + 1),
            Pick::Group(g) => {
                let Some(group) = scene.groups.get(g) else { continue };
                (g, 0..group.shapes.len())
            }
            _ => continue,
        };
        for s in range {
            let t = (g, s);
            if seen.insert(t) {
                out.push(t);
            }
        }
    }
    out
}

/// What a hit of the CPU field stands for: a shape, or (when `s` is `sdf::LINKED`) a whole linked copy.
pub fn as_pick(g: usize, s: usize) -> Pick {
    if s == crate::sdf::LINKED { Pick::Group(g) } else { Pick::Shape(g, s) }
}

/// Where the handles sit for several picks: the middle of all their shapes.
pub fn centre_of_picks(scene: &Scene, picks: &[Pick]) -> Option<V3> {
    if let [one] = picks {
        return centre_of(scene, Some(*one));
    }
    let t = targets_all(scene, picks);
    let mut sum = [0.0; 3];
    for (g, s) in &t {
        sum = add(sum, scene.shape(*g, *s)?.pos);
    }
    (!t.is_empty()).then(|| mul(sum, 1.0 / t.len() as f32))
}

/// Where the handles sit: the shape, or the middle of a group.
pub fn centre_of(scene: &Scene, pick: Option<Pick>) -> Option<V3> {
    // a linked copy has no shapes: its handles sit on the middle of the source, moved with the copy
    if let Some(Pick::Group(g)) = pick
        && let Some(gr) = scene.groups.get(g)
        && gr.is_linked()
    {
        let src = crate::pattern::link_source(scene, g)?;
        let c = crate::pattern::centre_of_shapes(&scene.groups.get(src)?.shapes);
        return Some(add(c, gr.xform.pos));
    }
    let t = targets(scene, pick);
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for (g, s) in &t {
        let sh = scene.shape(*g, *s)?;
        sum = add(sum, sh.pos);
        n += 1.0;
    }
    (n > 0.0).then(|| mul(sum, 1.0 / n))
}

/// A rough bounding radius of a shape, for framing the view.
pub fn radius_of(s: &crate::model::Shape) -> f32 {
    let m = s.scale[0].max(s.scale[1]).max(s.scale[2]);
    let r = match s.kind {
        Kind::Sphere => s.size[0],
        Kind::Box => len(s.size) * 0.5,
        Kind::Cylinder | Kind::Cone => len([s.size[0], s.size[1] * 0.5, 0.0]),
        Kind::Torus => s.size[0] + s.size[1],
        Kind::Extrude => {
            let w = s.outline.iter().map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt()).fold(0.0_f32, f32::max);
            len([w, s.size[0] * 0.5, 0.0])
        }
        _ => len(s.local_half()),
    };
    r * m
}

impl App {
    /// Draw the viewport and handle the mouse. Returns nothing: it edits `self`.
    pub fn viewport(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        self.view_rect = rect;
        let ppp = ui.ctx().pixels_per_point();
        let clean = self.capture.is_some();
        let now = ui.input(|i| i.time);
        let rev = self.track_scene();
        let scene_changed = self.perf.drawn.scene_changed(rev);

        let mods = ui.input(|i| i.modifiers);
        let pos = resp.hover_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()));

        // The curve's nodes come first: they win over the gizmo only where the mouse is on one
        let ct = if clean {
            crate::curve_tool::Input::default()
        } else if let Some(used) = self.sketch_input(ui, &resp, rect, pos, mods) {
            used
        } else {
            self.curve_input(ui, &resp, rect, pos, mods)
        };

        // The handles
        let tool = self.tool;
        let geom = if clean || self.ct.drawing || self.pt.on || self.sketch_active() {
            None
        } else {
            gizmo::build_multi(&self.cam, rect, &self.scene, &self.picks(), tool)
        };
        let over_gnomon = !clean && pos.is_some_and(|p| self.gnomon_hit(rect, p).is_some() || Self::home_hit(rect, p));
        let hover_handle = match (&geom, pos, &self.drag) {
            (Some(g), Some(p), None) if resp.hovered() && !over_gnomon && !ct.over => gizmo::hit(g, tool, p),
            _ => None,
        };
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if self.drag.is_some() {
                egui::CursorIcon::Grabbing
            } else if hover_handle.is_some() || over_gnomon || ct.over {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });
        }

        // The shape under the mouse lights up a little (looked up only when the mouse moved, 30 times a second at most).
        // During a hover transition we use the existing cheap picture, then refine after settling.
        let previous_hover = self.hover;
        let idle = self.drag.is_none() && !ui.input(|i| i.pointer.any_down());
        match (resp.hovered() && idle && hover_handle.is_none() && !over_gnomon, pos) {
            (true, Some(p)) => {
                let dirty = scene_changed || crate::perf::cam_key(&self.cam) != self.perf.last_cam;
                match self.perf.hover_gate.check([p.x, p.y], now, dirty) {
                    Gate::Pick => self.hover = self.shape_at(rect, p).map(|(g, s)| as_pick(g, s)),
                    Gate::Wait(w) => ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(w)),
                    Gate::Same => {}
                }
            }
            _ => {
                self.hover = None;
                self.perf.hover_gate.reset();
            }
        }

        self.perf.activity.hover_changed(now, previous_hover, self.hover);

        // Camera: middle-drag turns, right-drag slides, the wheel zooms toward the mouse
        let mut moved_camera = false;
        if resp.dragged_by(PointerButton::Middle) || (resp.dragged_by(PointerButton::Primary) && mods.alt && self.drag.is_none() && !ct.busy) {
            let d = resp.drag_delta();
            if mods.shift {
                self.cam.pan(d.x, d.y, rect.height());
            } else if mods.ctrl {
                if let Some(p) = pos {
                    self.cam.zoom_at(rect, p, -d.y * 2.0);
                }
            } else {
                self.cam.orbit(d.x, d.y);
            }
            moved_camera = true;
        }
        if resp.dragged_by(PointerButton::Secondary) {
            let d = resp.drag_delta();
            self.cam.pan(d.x, d.y, rect.height());
            moved_camera = true;
        }
        if resp.hovered() {
            // learn what the person is using from how the scrolling arrives
            let seen = ui.input(|i| {
                i.events.iter().rev().find_map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => device_of_scroll(*unit, *delta),
                    egui::Event::Zoom(_) => Some(Device::Trackpad),
                    _ => None,
                })
            });
            if let Some(d) = seen {
                self.input_seen = d;
            }
            let device = self.input_dev.device(self.input_seen);
            let (scroll_v, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            // trackpad: two fingers turn the view (Shift: slide it); the wheel of a mouse zooms
            let (scroll, turn) = if device == Device::Trackpad { (0.0, scroll_v) } else { (scroll_v.y, egui::Vec2::ZERO) };
            // egui folds a Shift+scroll into a sideways-only scroll: read the raw events so that
            // Shift + two fingers slides the view up/down as well as left/right
            let turn = if device == Device::Trackpad && mods.shift {
                let raw = ui.input(|i| {
                    i.events.iter().fold(egui::Vec2::ZERO, |acc, e| match e {
                        egui::Event::MouseWheel { unit, delta, .. } => {
                            acc + match unit {
                                egui::MouseWheelUnit::Point => *delta,
                                _ => *delta * 24.0,
                            }
                        }
                        _ => acc,
                    })
                });
                if raw != egui::Vec2::ZERO { raw } else { turn }
            } else {
                turn
            };
            if turn != egui::Vec2::ZERO && turn.x.is_finite() && turn.y.is_finite() {
                if mods.shift {
                    self.cam.pan(turn.x, turn.y, rect.height());
                } else {
                    self.cam.orbit(-turn.x, -turn.y);
                }
                moved_camera = true;
            }
            if let Some(p) = pos {
                // Apply wheel and pinch in one step, keeping the cursor anchor stable.
                // Keep the original pinch sensitivity and mouse bindings.
                let pinch_scroll = if pinch.is_finite() && pinch > 0.0 { pinch.ln() * 660.0 } else { 0.0 };
                let total_scroll = scroll + pinch_scroll;
                if total_scroll.is_finite() && total_scroll != 0.0 {
                    self.cam.zoom_at(rect, p, total_scroll);
                    moved_camera = true;
                }
            }
        }

        // Box selection (when one of the two box buttons is on) takes drags that start on empty space
        // a press on a handle (or the axis ball) is never a box: judge by where the button went down
        let press = ui.input(|i| i.pointer.press_origin());
        let press_on_handle = matches!((&geom, press), (Some(g), Some(o)) if gizmo::hit(g, tool, o).is_some());
        let press_on_gnomon = press.is_some_and(|o| self.gnomon_hit(rect, o).is_some() || Self::home_hit(rect, o));
        let marquee_free = hover_handle.is_none() && !press_on_handle && !press_on_gnomon && !ct.taken && !ct.over && !over_gnomon;
        let marquee_taken = self.marquee_input(ui, &resp, rect, marquee_free, mods.shift);
        // A drag that starts on a handle moves the thing; a drag that starts anywhere else turns the view
        if resp.drag_started_by(PointerButton::Primary) && !mods.alt && !ct.taken && !marquee_taken && !self.sketch_active() {
            // where the button went down (the pointer has moved a little by now)
            let origin = ui.input(|i| i.pointer.press_origin()).or(pos);
            let grabbed = match (&geom, origin) {
                (Some(g), Some(o)) => gizmo::hit(g, tool, o).map(|h| (g, o, h)),
                _ => None,
            };
            if let Some((g, o, h)) = grabbed {
                self.drag = Some(self.begin_drag(h, g, rect, o, tool, mods.shift));
                self.orbiting = false;
            } else {
                self.orbiting = origin.is_some_and(|o| self.gnomon_hit(rect, o).is_none() && !Self::home_hit(rect, o));
            }
        }
        if self.sketch_active() {
            // A pending orbit from before the tool was enabled must never
            // leak into a sketch gesture, including when the modifier changes.
            self.orbiting = false;
        }
        if self.orbiting && resp.dragged_by(PointerButton::Primary) && self.drag.is_none() && !mods.alt && !ct.busy {
            let d = resp.drag_delta();
            if mods.shift {
                self.cam.pan(d.x, d.y, rect.height());
            } else {
                self.cam.orbit(d.x, d.y);
            }
            moved_camera = true;
        }
        if let Some(mut d) = self.drag.take() {
            if resp.dragged_by(PointerButton::Primary)
                && let Some(p) = ui.input(|i| i.pointer.interact_pos())
            {
                self.apply_drag(&mut d, p, mods.ctrl);
            }
            if !resp.drag_stopped() && ui.input(|i| i.pointer.primary_down()) {
                self.drag = Some(d);
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.orbiting = false;
        }
        if moved_camera {
            self.fly = None;
        }

        // Clicks
        if resp.clicked_by(PointerButton::Primary)
            && hover_handle.is_none()
            && !ct.taken
            && !self.sketch_active()
            && let Some(p) = pos
        {
            if Self::home_hit(rect, p) {
                self.go_home();
            } else if let Some(axis) = self.gnomon_hit(rect, p) {
                self.look_along(axis);
            } else {
                if mods.shift {
                    self.toggle_at(rect, p);
                } else {
                    // Ctrl/Cmd toggles this item. Use the scene tree to select a whole group.
                    self.pick_at(rect, p, mods.command || mods.ctrl, mods.command || mods.ctrl);
                }
            }
        }
        // while a drawing tool is on, the right click belongs to the tool (it confirms)
        let tool_busy = ct.taken || self.sketch_active() || self.ct.drawing || self.pt.on;
        if resp.secondary_clicked()
            && !tool_busy
            && let Some(p) = pos
        {
            let under = self.guide_at(rect, p).or_else(|| self.shape_at(rect, p)).map(|(g, s)| as_pick(g, s));
            if !under.is_some_and(|item| self.is_pick_selected(item)) {
                self.select_pick(under, false);
            }
        }
        if !tool_busy {
            resp.context_menu(|ui| self.context_menu(ui));
        }
        if resp.double_clicked_by(PointerButton::Primary)
            && hover_handle.is_none()
            && !ct.taken
            && !self.sketch_active()
            && let Some(p) = pos
            && self.gnomon_hit(rect, p).is_none()
            && !Self::home_hit(rect, p)
        {
            match self.shape_at(rect, p) {
                Some((g, s)) if self.scene.shape(g, s).is_some_and(crate::sketch::editable) => {
                    self.sketch_edit_begin(g, s);
                }
                _ => self.frame_view(self.sel.is_some()),
            }
        }

        // Input can change geometry *after* the revision noted at the start of this frame.
        // Use the resulting revision for the scene pack, so a drag/Undo never renders stale data.
        let render_rev = self.track_scene();
        // The picture itself. It is made after the input above so that it shows this frame's state.
        // Fast mode (cheap shading, fewer pixels) runs while the view or a handle moves, and for a
        // moment after; then one full-quality frame follows.
        let cam_now = crate::perf::cam_key(&self.cam);
        let editing_drag = self.drag.is_some() || ct.busy || self.sk.dragging
            || self.sk.edit.as_ref().is_some_and(|e| e.drag.is_some());
        if cam_now != self.perf.last_cam || editing_drag
            || ((render_rev != rev || scene_changed) && ui.input(|i| i.pointer.any_down())) {
            self.perf.activity.touch(now);
        }
        self.perf.last_cam = cam_now;
        let fast = !clean && self.perf.activity.is_fast(now);
        if let Some(wait) = self.perf.activity.settle_in(now)
            && !clean
        {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        let vp = crate::perf::viewport_px(rect, ppp);
        let (scale_moving, scale_still) = if clean { (1.0, 1.0) } else { self.picture_scales(now, fast) };
        let shown_sel = if clean { None } else { self.sel };
        let shown_hover = if clean { None } else { self.hover };
        let mut sharp = 1.0;
        let mut counts = (self.scene.shape_count(), self.scene.groups.len(), 0);
        let mut scale_shown = 1.0;
        if crate::gpu_ui::use_gpu(&self.gpu.status.snapshot(), self.gpu.easy) {
            let pack = self.perf.pack.get(&self.scene, render_rev);
            let mut globals = pack.globals(&self.scene, &self.cam, [vp[0] as f32, vp[1] as f32], shown_sel, shown_hover, self.srgb);
            crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(self.srgb, false, pack.glow));
            crate::gpu::set_view_mode(&mut globals, self.view_mode.id());
            counts = (self.scene.shape_count(), pack.group_n, pack.instances);
            let fbs = self.perf.feedback.state();
            sharp = if fast { 0.0 } else { fbs.sharp };
            scale_shown = if fast { scale_moving } else { scale_still };
            let cb = crate::gpu::Callback {
                globals,
                pack,
                viewport: vp,
                fast,
                scale_moving,
                scale_still,
                sharpen: self.perf.sharpen,
                whole: clean,
                frame_ms: self.perf.stats.last_interval_ms(),
                status: self.gpu.status.clone(),
                feedback: self.perf.feedback.clone(),
                ctx: ui.ctx().clone(),
            };
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, cb));
        } else {
            // the simple view: drawn on the CPU by a worker thread, never blocking the window
            self.gpu.easy_view.mode = self.view_mode.id();
            self.gpu.easy_view.update(ui.ctx(), &self.scene, &self.cam, vp, fast);
            self.gpu.easy_view.paint(&ui.painter_at(rect), rect, self.scene.env.backdrop);
        }
        self.perf.last_scale = scale_moving;
        self.perf.last_fast = fast;
        self.perf.drawn.note(render_rev, shown_sel, shown_hover, &self.cam, clean);
        if self.perf.hud && !clean {
            let fbs = self.perf.feedback.state();
            let info = crate::hud::Context {
                shapes: counts.0,
                groups: counts.1,
                instances: counts.2,
                scale: scale_shown,
                adapter: self.gpu.status.snapshot().info.short(),
                gpu_timing: fbs.timing,
                fast,
                sharp,
                pixels: crate::perf::render_size(vp, scale_shown, 16384),
                quality: self.perf.quality.label().to_string(),
            };
            crate::hud::show(ui.ctx(), rect, &self.perf.stats.summary(now), &info);
        }
        let painter = ui.painter_at(rect);

        // Draw the handles over the picture
        if clean {
            return;
        }
        self.gpu_cards(ui.ctx(), rect);
        // Input can move the camera or shape earlier this frame. Draw handles against
        // the updated positions rather than the stale pre-input geometry.
        let draw_geom = if clean || self.ct.drawing || self.pt.on || self.sketch_active() {
            None
        } else if moved_camera || self.drag.is_some() {
            gizmo::build_multi(&self.cam, rect, &self.scene, &self.picks(), tool)
        } else {
            geom
        };
        if let Some(g) = &draw_geom {
            let active = self.drag.as_ref().map(gizmo::Drag::handle).or(hover_handle);
            gizmo::draw(&painter, g, tool, active);
        }
        self.guide_paint(&painter, rect);
        self.curve_paint(&painter, rect);
        self.pipe_paint(&painter, rect);
        self.sketch_paint(&painter, rect);
        self.marquee_paint(&painter);
        self.paint_multi_selection(&painter, rect);
        self.draw_gnomon(&painter, rect, pos);
    }

    /// The render scales for this frame: the cheap picture while moving, the sharp one when still.
    /// Feeds the controller with what the last frames cost (graphics card timings when the device
    /// gives them, else the time between frames).
    fn picture_scales(&mut self, now: f64, fast: bool) -> (f32, f32) {
        let passes = self.perf.feedback.take_passes();
        let mut timed = false;
        for p in &passes {
            self.perf.stats.gpu(now, p.ms);
            if p.kind == crate::gpu_timing::Kind::Moving {
                self.perf.scale.observe(crate::adapt::Sample::Gpu { ms: p.ms, scale: p.scale });
                timed = true;
            }
        }
        // no timestamps: while moving, the time between frames is the best sign of the picture's cost
        let interval = self.perf.stats.last_interval_ms();
        if !timed && !self.perf.feedback.state().timing && fast && self.perf.last_fast && interval > 0.0 {
            self.perf.scale.observe(crate::adapt::Sample::Frame { ms: interval, scale: self.perf.last_scale, cpu_ms: self.perf.stats.prev_cpu_ms() });
        }
        let q = self.perf.quality;
        (q.moving_scale().unwrap_or_else(|| self.perf.scale.scale()), q.still_scale())
    }

    fn gnomon_items(&self, rect: Rect) -> Vec<(V3, Pos2, bool)> {
        let origin = pos2(rect.right() - 62.0, rect.top() + 62.0);
        let (right, up, _) = self.cam.basis();
        let mut v = Vec::new();
        for i in 0..3 {
            let mut a = [0.0; 3];
            a[i] = 1.0;
            for (sign, pos_end) in [(1.0, true), (-1.0, false)] {
                let d = mul(a, sign);
                let p = origin + vec2(dot(d, right), -dot(d, up)) * 34.0;
                v.push((d, p, pos_end));
            }
        }
        v
    }

    fn gnomon_hit(&self, rect: Rect, p: Pos2) -> Option<V3> {
        self.gnomon_items(rect)
            .into_iter()
            .filter(|(_, c, positive)| *positive && (*c - p).length() < 10.0)
            .map(|(d, _, _)| d)
            .next()
            .or_else(|| self.gnomon_items(rect).into_iter().find(|(_, c, _)| (*c - p).length() < 8.0).map(|(d, _, _)| d))
    }

    /// The home button sits at the upper left of the axis ball.
    fn home_centre(rect: Rect) -> Pos2 {
        pos2(rect.right() - 62.0 - 44.0, rect.top() + 62.0 - 44.0)
    }

    fn home_hit(rect: Rect, p: Pos2) -> bool {
        (Self::home_centre(rect) - p).length() < HOME_RADIUS + 2.0
    }

    /// The home view: the first angle, everything in sight (gliding there).
    pub fn go_home(&mut self) {
        self.frame_view(false);
        let home = Camera::default();
        if let Some(goal) = self.fly.as_mut() {
            goal.yaw = home.yaw;
            goal.pitch = home.pitch;
        }
    }

    fn draw_home(&self, p: &egui::Painter, rect: Rect, mouse: Option<Pos2>) {
        let c = Self::home_centre(rect);
        let hot = mouse.is_some_and(|m| Self::home_hit(rect, m));
        p.circle_filled(c, HOME_RADIUS, Color32::from_black_alpha(if hot { 90 } else { 54 }));
        let ink = if hot { style::TEXT } else { style::DIM };
        let s = Stroke::new(1.6, ink);
        // a little house: roof and walls
        let roof = [c + vec2(-6.5, -0.5), c + vec2(0.0, -6.5), c + vec2(6.5, -0.5)];
        p.line_segment([roof[0], roof[1]], s);
        p.line_segment([roof[1], roof[2]], s);
        p.line_segment([c + vec2(-4.5, -1.0), c + vec2(-4.5, 5.5)], s);
        p.line_segment([c + vec2(4.5, -1.0), c + vec2(4.5, 5.5)], s);
        p.line_segment([c + vec2(-4.5, 5.5), c + vec2(4.5, 5.5)], s);
    }

    fn draw_gnomon(&self, p: &egui::Painter, rect: Rect, mouse: Option<Pos2>) {
        self.draw_home(p, rect, mouse);
        let origin = pos2(rect.right() - 62.0, rect.top() + 62.0);
        p.circle_filled(origin, 48.0, Color32::from_black_alpha(36));
        let mut items = self.gnomon_items(rect);
        let (_, _, fwd) = self.cam.basis();
        // far ones first
        items.sort_by(|a, b| dot(a.0, fwd).total_cmp(&dot(b.0, fwd)).reverse());
        for (d, c, positive) in items {
            let axis = (0..3).find(|&i| d[i].abs() > 0.5).unwrap_or(0);
            let col = style::AXIS[axis];
            let hot = mouse.is_some_and(|m| (m - c).length() < 10.0);
            if positive {
                p.line_segment([origin, c], Stroke::new(2.0, col));
                p.circle_filled(c, if hot { 10.0 } else { 8.5 }, col);
                p.text(c, Align2::CENTER_CENTER, ["X", "Y", "Z"][axis], FontId::proportional(10.5), Color32::from_black_alpha(200));
            } else {
                p.circle_filled(c, if hot { 7.0 } else { 5.5 }, col.gamma_multiply(0.45));
            }
        }
    }

    /// Look straight along a world axis (clicking the gnomon).
    pub fn look_along(&mut self, d: V3) {
        // d points from the centre toward the viewer
        let pitch = d[2].clamp(-1.0, 1.0).asin();
        let yaw = if d[2].abs() > 0.9 { self.cam.yaw } else { d[0].atan2(-d[1]) };
        self.fly_look(yaw, pitch);
    }

    /// Turn the view to look from (yaw, pitch), gliding there.
    pub fn fly_look(&mut self, yaw: f32, pitch: f32) {
        let mut goal = self.fly.unwrap_or(self.cam);
        goal.look(yaw, pitch);
        self.fly = Some(goal);
    }

    /// One step of the glide toward `self.fly`; true while it is still moving.
    pub fn step_fly(&mut self, dt: f32) -> bool {
        let Some(goal) = self.fly else { return false };
        let k = 1.0 - (-dt.clamp(0.0, 0.1) * 13.0).exp();
        let c = &mut self.cam;
        c.target = add(c.target, mul(sub(goal.target, c.target), k));
        c.dist = (c.dist.ln() + (goal.dist.ln() - c.dist.ln()) * k).exp();
        let mut dy = goal.yaw - c.yaw;
        while dy > std::f32::consts::PI {
            dy -= std::f32::consts::TAU;
        }
        while dy < -std::f32::consts::PI {
            dy += std::f32::consts::TAU;
        }
        c.yaw += dy * k;
        c.pitch += (goal.pitch - c.pitch) * k;
        let done = len(sub(goal.target, c.target)) < 0.02
            && (goal.dist / c.dist - 1.0).abs() < 0.002
            && dy.abs() < 0.002
            && (goal.pitch - c.pitch).abs() < 0.002;
        if done {
            *c = Camera { ortho: c.ortho, ..goal };
            self.fly = None;
        }
        !done
    }

    /// The shape under a screen point, if any.
    pub fn shape_at(&self, rect: Rect, p: Pos2) -> Option<(usize, usize)> {
        let (o, d) = self.cam.ray_at(rect, p);
        self.field().ray(o, d, 5000.0).and_then(|(_, s)| s.nearest)
    }

    /// Shift + click: add the thing under the point to the selection, or take it out again.
    fn toggle_at(&mut self, rect: Rect, p: Pos2) {
        if let Some((g, s)) = self.shape_at(rect, p) {
            self.select_pick(Some(as_pick(g, s)), true);
        }
    }

    fn pick_at(&mut self, rect: Rect, p: Pos2, keep: bool, additive: bool) {
        // a sketch line near the click wins over what lies behind it
        let hit = self.guide_at(rect, p).or_else(|| self.shape_at(rect, p));
        match hit {
            Some((g, s)) => {
                let want = as_pick(g, s);
                let pick = if keep && !additive && self.sel == Some(want) { Pick::Group(g) } else { want };
                self.select_pick(Some(pick), additive && pick == want);
            }
            None if !additive => self.select_pick(None, false),
            None => {}
        }
    }

    pub fn frame_view(&mut self, only_selected: bool) {
        let spheres: Vec<crate::accel::Bound> = if only_selected && self.sel.is_some() {
            let t = targets(&self.scene, self.sel);
            t.iter()
                .filter_map(|(g, s)| self.scene.shape(*g, *s))
                .filter(|s| s.visible)
                .map(|s| crate::accel::Bound { c: s.pos, r: radius_of(s) })
                .collect()
        } else {
            crate::pattern::world_spheres(&self.scene)
        };
        if spheres.is_empty() {
            self.fly = Some(Camera { ortho: self.cam.ortho, ..Camera::default() });
            return;
        }
        let mut c = [0.0; 3];
        for b in &spheres {
            c = add(c, b.c);
        }
        let c = mul(c, 1.0 / spheres.len() as f32);
        let r = spheres.iter().map(|b| len(sub(b.c, c)) + b.r).fold(1.0_f32, f32::max);
        let mut goal = self.cam;
        goal.frame(c, r);
        self.fly = Some(goal);
    }

    /// Frame everything at once (no glide): for a freshly opened file.
    pub fn frame_now(&mut self) {
        self.frame_view(false);
        if let Some(g) = self.fly.take() {
            self.cam = g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_pick_covers_all_its_shapes() {
        let mut s = Scene::default();
        let extra = s.groups[0].shapes[0].clone();
        s.groups[0].shapes.push(extra);
        assert_eq!(targets(&s, Some(Pick::Group(0))).len(), 2);
        assert_eq!(targets(&s, Some(Pick::Shape(0, 5))).len(), 0);
    }

    fn reference_targets(scene: &Scene, picks: &[Pick]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for pick in picks {
            for target in targets(scene, Some(*pick)) {
                if !out.contains(&target) {
                    out.push(target);
                }
            }
        }
        out
    }

    fn selection_scene(count: usize) -> Scene {
        let mut scene = Scene::default();
        let base = scene.groups[0].shapes[0].clone();
        scene.groups[0].shapes = (0..count)
            .map(|i| {
                let mut shape = base.clone();
                shape.pos[0] = i as f32;
                shape
            })
            .collect();
        scene
    }

    #[test]
    fn overlapping_group_and_shape_picks_preserve_first_encounter_order() {
        let mut scene = selection_scene(4);
        let mut other = scene.groups[0].clone();
        other.shapes.truncate(2);
        scene.groups.push(other);
        let picks = [
            Pick::Shape(0, 2),
            Pick::Group(1),
            Pick::Group(usize::MAX),
            Pick::Group(0),
            Pick::Shape(1, 0),
            Pick::Shape(0, usize::MAX),
            Pick::Group(1),
        ];
        let want = vec![(0, 2), (1, 0), (1, 1), (0, 0), (0, 1), (0, 3)];
        assert_eq!(targets_all(&scene, &picks), want);
        assert_eq!(targets_all(&scene, &picks), reference_targets(&scene, &picks));
        assert_eq!(targets_all(&scene, &[]), Vec::new());
        assert_eq!(targets_all(&scene, &[Pick::Group(0)]), targets(&scene, Some(Pick::Group(0))));
        // The handle centre depends on the same once-only list, not on how many
        // overlapping group and shape picks refer to each object.
        let expected_x = want.iter().map(|&(g, s)| scene.groups[g].shapes[s].pos[0]).sum::<f32>() / want.len() as f32;
        assert!(centre_of_picks(&scene, &picks).is_some_and(|p| (p[0] - expected_x).abs() < 1e-5));
    }

    #[test]
    fn large_multi_selections_match_the_original_targets_exactly() {
        for count in [10, 100, 300, 1000] {
            let scene = selection_scene(count);
            let mut picks: Vec<_> = (0..count).rev().map(|s| Pick::Shape(0, s)).collect();
            picks.extend([Pick::Group(0), Pick::Shape(usize::MAX, 0), Pick::Group(0)]);
            let selected = targets_all(&scene, &picks);
            assert_eq!(selected.len(), count);
            assert_eq!(selected, reference_targets(&scene, &picks));
        }
    }

    /// CPU selection bookkeeping only, not viewport FPS. Compares the exact old
    /// algorithm with the same picks and preserves the returned order.
    #[test]
    #[ignore]
    fn multi_selection_speed() {
        use std::hint::black_box;
        eprintln!("CPU selection expansion; medians of 7 interleaved runs");
        eprintln!("shapes,reference_us,optimized_us,ratio");
        for count in [10, 100, 300, 1000] {
            let scene = selection_scene(count);
            let picks: Vec<_> = (0..count).map(|s| Pick::Shape(0, s)).collect();
            assert_eq!(targets_all(&scene, &picks), reference_targets(&scene, &picks));
            let repeat = 30000 / count;
            let measure = |optimized| {
                let started = std::time::Instant::now();
                for _ in 0..repeat {
                    black_box(if optimized {
                        targets_all(black_box(&scene), black_box(&picks))
                    } else {
                        reference_targets(black_box(&scene), black_box(&picks))
                    });
                }
                started.elapsed().as_secs_f64() * 1e6 / repeat as f64
            };
            let mut times: [Vec<f64>; 2] = Default::default();
            for run in 0..8 {
                for step in 0..2 {
                    let which = (run + step) % 2;
                    let elapsed = measure(which == 1);
                    if run > 0 {
                        times[which].push(elapsed);
                    }
                }
            }
            for row in &mut times {
                row.sort_by(f64::total_cmp);
            }
            let [reference, optimized] = times.map(|row| row[3]);
            eprintln!("{count},{reference:.3},{optimized:.3},{:.2}", reference / optimized);
        }
    }

    #[test]
    fn the_view_glides_to_its_goal() {
        let mut app = App::new(true, None);
        app.frame_view(false);
        let goal = app.fly.map(|g| g.dist);
        assert!(goal.is_some());
        for _ in 0..200 {
            app.step_fly(0.016);
        }
        assert!(app.fly.is_none());
        assert!(goal.is_some_and(|g| (app.cam.dist - g).abs() < 0.5));
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use egui::{MouseWheelUnit as U, vec2};

    #[test]
    fn the_scroll_shape_tells_wheel_from_trackpad() {
        assert_eq!(device_of_scroll(U::Line, vec2(0.0, 1.0)), Some(Device::Mouse));
        assert_eq!(device_of_scroll(U::Line, vec2(0.0, -3.0)), Some(Device::Mouse));
        assert_eq!(device_of_scroll(U::Line, vec2(0.0, 0.37)), Some(Device::Trackpad));
        assert_eq!(device_of_scroll(U::Point, vec2(2.0, 5.0)), Some(Device::Trackpad));
        assert_eq!(device_of_scroll(U::Point, vec2(0.0, 0.0)), None);
        assert_eq!(device_of_scroll(U::Point, vec2(f32::NAN, 1.0)), None);
    }

    #[test]
    fn a_fixed_choice_beats_what_was_seen() {
        assert_eq!(InputDev::Auto.device(Device::Trackpad), Device::Trackpad);
        assert_eq!(InputDev::Mouse.device(Device::Trackpad), Device::Mouse);
        assert_eq!(InputDev::Trackpad.device(Device::Mouse), Device::Trackpad);
    }
}

#[cfg(test)]
mod home_tests {
    use super::*;

    #[test]
    fn home_goes_to_the_first_angle_and_frames_everything() {
        let mut app = App::new(true, None);
        app.cam.yaw = 2.0;
        app.cam.pitch = -0.4;
        app.go_home();
        let goal = app.fly.expect("the view glides home");
        let first = Camera::default();
        assert_eq!((goal.yaw, goal.pitch), (first.yaw, first.pitch));
        assert!(goal.dist > 0.0 && goal.dist.is_finite());
    }

    #[test]
    fn the_home_button_is_at_the_upper_left_of_the_axis_ball_and_only_hits_itself() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0));
        let c = App::home_centre(rect);
        assert!(App::home_hit(rect, c));
        assert!(!App::home_hit(rect, c + vec2(40.0, 0.0)));
        let ball = pos2(rect.right() - 62.0, rect.top() + 62.0);
        assert!(c.x < ball.x && c.y < ball.y, "up and to the left of the ball");
        assert!((c - ball).length() > 48.0 + HOME_RADIUS, "beside the ball, not on it");
        assert!(rect.contains(c));
    }
}
