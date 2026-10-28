//! Whole-viewport input tests: real pointer events go through `App::viewport`, the way a mouse
//! does in the window (the unit tests of each tool call its methods directly and cannot see
//! problems in how the viewport hands the mouse to them).

use egui::{Event, PointerButton, Pos2, RawInput, Rect, Vec2, pos2, vec2};

use crate::app::App;
use crate::curve_tool::layout;
use crate::model::{Kind, Material, Pick, Shape};
use crate::sketch::{self, Frame};
use crate::sketch_ui::{Pen, Stage};

struct Rig {
    ctx: egui::Context,
    app: App,
    time: f64,
    size: Vec2,
    mods: egui::Modifiers,
    full_ui: bool,
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        crate::style::apply(&ctx);
        let mut rig = Rig { ctx, app: App::new(true, None), time: 0.0, size: vec2(1100.0, 760.0), mods: egui::Modifiers::NONE, full_ui: false };
        rig.frame(vec![]);
        rig.frame(vec![]);
        rig
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.016;
        let mut all = vec![Event::ModifiersChanged(self.mods)];
        all.extend(events);
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), events: all, time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        let full_ui = self.full_ui;
        let mut out = self.ctx.run_ui(input, |ui| {
            if full_ui {
                app.frame_body(ui);
                return;
            }
            // Match frame_body ordering: keys, canvas, then the interactive overlay.
            app.keys(ui.ctx());
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| app.viewport(ui));
            app.sketch_card(ui.ctx());
        });
        out.textures_delta.clear();
        if !full_ui {
            self.app.settle(&self.ctx);
        }
    }

    /// Use the real window-thread control queue, including commands batched in one frame.
    fn control(&mut self, commands: &[serde_json::Value]) -> Vec<Result<serde_json::Value, String>> {
        let (send, requests) = std::sync::mpsc::channel();
        self.app.control = Some(requests);
        // one answer channel per command: a picture is answered later than the others
        let answers: Vec<_> = commands
            .iter()
            .map(|cmd| {
                let (reply, answer) = std::sync::mpsc::channel();
                send.send(crate::control::Request { cmd: cmd.clone(), reply }).expect("queue the control command");
                answer
            })
            .collect();
        let full_ui = self.full_ui;
        self.full_ui = true;
        self.frame(vec![]);
        self.full_ui = full_ui;
        // a picture is drawn on a worker: keep drawing frames until its answer arrives
        answers
            .iter()
            .map(|answer| {
                for _ in 0..600 {
                    if let Ok(result) = answer.try_recv() {
                        return result;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    // the full frame is the one that collects a finished picture
                    let full_ui = self.full_ui;
                    self.full_ui = true;
                    self.frame(vec![]);
                    self.full_ui = full_ui;
                }
                Err("the frame never replied to the command".into())
            })
            .collect()
    }

    fn right_click(&mut self, p: Pos2) {
        self.mv(p);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: self.mods }]);
        }
        self.frame(vec![]);
    }

    fn mv(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn button(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: self.mods }]);
    }

    fn click(&mut self, p: Pos2) {
        self.mv(p);
        self.button(p, true);
        self.button(p, false);
        self.frame(vec![]);
    }

    fn key(&mut self, key: egui::Key) {
        self.frame(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: self.mods }]);
        self.frame(vec![Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: self.mods }]);
        self.frame(vec![]);
    }

    /// Enter through the same default as the visible Sketch button.
    fn sketch(&mut self, pen: Pen) {
        self.app.set_sketch_tool(true);
        self.prepare_sketch(pen);
    }

    fn plain_sketch(&mut self, pen: Pen) {
        self.sketch(pen);
    }

    fn saved_rectangle(&mut self) -> (usize, usize) {
        self.plain_sketch(Pen::Rect);
        self.click(self.on_plane([-25.0, -15.0]));
        self.click(self.on_plane([5.0, 5.0]));
        self.right_click(self.on_plane([5.0, 5.0]));
        let Some(Pick::Shape(g, s)) = self.app.sel else { panic!("the saved sketch is selected") };
        assert!(self.app.scene.shape(g, s).is_some_and(|sh| sh.guide));
        (g, s)
    }

    fn row_rect(&self, pick: Pick) -> Rect {
        self.app.row_rects.iter().find_map(|(id, rect)| (*id == pick).then_some(*rect)).expect("the scene row is visible")
    }

    /// Exercise the retained direct-solid path separately from the public Sketch workflow.
    fn direct_solid_sketch(&mut self, pen: Pen) {
        self.app.set_sketch_tool(true);
        self.app.sk.keep_sketch = false;
        self.prepare_sketch(pen);
    }

    fn prepare_sketch(&mut self, pen: Pen) {
        // (the plane is chosen by name: the pick-a-plane gesture has its own tests)
        self.app.sketch_set_plane(crate::sketch_ui::PlaneSel::Floor);
        self.app.sk.pen = pen;
        self.app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        self.app.cam.target = [0.0, 0.0, 0.0];
        self.app.cam.dist = 140.0;
        self.app.fly = None;
        // Populate the card's hit-test layer before sending input events.
        self.frame(vec![]);
        self.frame(vec![]);
    }

    /// Replace the focused field through the actual TextEdit input path.
    fn replace_text(&mut self, text: &str) {
        assert!(self.ctx.memory(|m| m.focused()).is_some(), "a numeric field must own the text input");
        let previous = self.mods;
        self.mods = egui::Modifiers { ctrl: true, command: true, ..egui::Modifiers::NONE };
        self.key(egui::Key::A);
        self.mods = previous;
        self.frame(vec![Event::Text(text.to_string())]);
        self.frame(vec![]);
    }

    fn on_plane(&self, q: [f32; 2]) -> Pos2 {
        self.app.cam.project(self.app.view_rect, Frame::floor().to_world(q)).expect("point in viewport")
    }

    /// Press at `from`, move in steps to `to`, release.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.mv(from);
        self.button(from, true);
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            self.mv(from + (to - from) * t);
        }
        self.button(to, false);
        self.frame(vec![]);
        self.frame(vec![]);
    }
}

#[test]
fn organic_tube_control_point_drags_through_real_egui_pointer_events() {
    let mut r = Rig::new();
    r.app.add_organic_tube(&r.ctx, crate::organic_tube::Preset::Wave);
    // The application animates frame_view over time; the viewport-only test
    // rig does not run frame_body/step_fly, so take the camera goal directly.
    if let Some(goal) = r.app.fly.take() {
        r.app.cam = goal;
    }
    r.frame(vec![]);
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("Organic Tube selected") };
    let before = r.app.scene.shape(g, s).expect("tube").path[3];
    let layout = layout(&r.app.cam, r.app.view_rect, r.app.scene.shape(g, s).expect("tube"));
    let node = layout.nodes[3].expect("visible control point");
    let before_cam = (r.app.cam.yaw, r.app.cam.pitch);
    r.drag(node.c, node.c + vec2(45.0, -20.0));
    let after = r.app.scene.shape(g, s).expect("tube").path[3];
    assert_ne!(after[..3], before[..3], "node must move on the native curve");
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), before_cam, "node drag must not orbit the camera");
    let text = r.app.scene.to_json();
    let saved = crate::model::Scene::from_json(&text).expect("native save");
    assert_eq!(saved.shape(g, s).expect("tube").path[3], after);
}

fn curve_shape() -> Shape {
    let mut s = Shape::new(Kind::Curve, [0.0, 0.0, 0.0], Material::default());
    s.path = vec![[-30.0, 0.0, 10.0, 5.0], [0.0, 0.0, 30.0, 5.0], [30.0, 0.0, 10.0, 5.0]];
    s
}

#[test]
fn a_node_of_a_selected_curve_can_be_dragged_with_the_mouse() {
    let mut r = Rig::new();
    r.app.scene.groups[0].shapes.push(curve_shape());
    r.app.sel = Some(Pick::Shape(0, 1));
    r.frame(vec![]);
    let rect = r.app.view_rect;
    let Some(shape) = r.app.scene.shape(0, 1) else { panic!("curve") };
    let lay = layout(&r.app.cam, rect, shape);
    let Some(Some(node)) = lay.nodes.get(1).copied() else { panic!("the middle node is on screen") };
    let before = shape.path.get(1).copied();
    r.drag(node.c, node.c + vec2(60.0, -30.0));
    let after = r.app.scene.shape(0, 1).and_then(|s| s.path.get(1).copied());
    assert_ne!(before, after, "dragging the node must move it (before {before:?}, after {after:?})");
}

#[test]
fn a_node_of_a_freshly_traced_pipe_can_be_dragged_too() {
    let mut r = Rig::new();
    // trace a pipe over the floor with the pipe tool
    r.app.pt.on = true;
    let rect = r.app.view_rect;
    let c = rect.center();
    r.mv(c + vec2(-150.0, 40.0));
    r.button(c + vec2(-150.0, 40.0), true);
    for k in 1..=24 {
        let t = k as f32 / 24.0;
        r.mv(c + vec2(-150.0 + 300.0 * t, 40.0 - 90.0 * (t * std::f32::consts::PI).sin()));
    }
    r.button(c + vec2(150.0, 40.0), false);
    r.frame(vec![]);
    r.frame(vec![]);
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the new pipe is selected, got {:?}", r.app.sel) };
    let Some(shape) = r.app.scene.shape(g, s) else { panic!("pipe") };
    assert_eq!(shape.kind, Kind::Curve, "the stroke made a curve");
    assert!(shape.path.len() >= 3, "{} nodes", shape.path.len());
    // switch the pipe tool off like a person would (Esc) and drag a middle node
    r.app.pt.on = false;
    r.frame(vec![]);
    let Some(shape) = r.app.scene.shape(g, s) else { panic!("pipe") };
    let lay = layout(&r.app.cam, r.app.view_rect, shape);
    let mid = shape.path.len() / 2;
    let Some(Some(node)) = lay.nodes.get(mid).copied() else { panic!("node on screen") };
    let before = shape.path.get(mid).copied();
    r.drag(node.c, node.c + vec2(0.0, -50.0));
    let after = r.app.scene.shape(g, s).and_then(|s| s.path.get(mid).copied());
    assert_ne!(before, after, "the traced pipe's node must move (before {before:?}, after {after:?})");
}

#[test]
fn clicking_a_ball_row_moves_that_ball_only_with_the_arrow_keys() {
    let mut r = Rig::new();
    r.app.scene.groups.clear();
    let mut group = crate::model::Group::new("Pipe");
    group.shapes.push(Shape::new(Kind::Sphere, [0.0, 0.0, 120.0], Material::default()));
    let mut pipe = Shape::new(Kind::Curve, [0.0, 0.0, 20.0], Material::default());
    pipe.name = "Pipe".into();
    pipe.path = vec![[-20.0, 0.0, 0.0, 4.0], [0.0, 0.0, 0.0, 4.0], [20.0, 0.0, 0.0, 4.0], [40.0, 0.0, 0.0, 4.0], [60.0, 0.0, 0.0, 4.0]];
    group.shapes.push(pipe);
    r.app.scene.groups.push(group);
    r.app.sel = Some(Pick::Shape(0, 1));
    r.app.committed_sync();

    // Draw the real scene list, then click the "Ball 4" row under the selected pipe.
    r.full_ui = true;
    r.frame(vec![]);
    r.frame(vec![]);
    let row = r.row_rect(Pick::Shape(0, 1));
    let ball_4 = pos2(row.left() + 110.0, row.bottom() + 2.0 + 3.0 * (24.0 + 2.0) + 12.0);
    r.click(ball_4);
    assert_eq!(r.app.ct.node, Some((0, 1, 3)), "clicking the row chooses Ball 4");
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 1)));
    assert_eq!(r.app.picks(), vec![Pick::Shape(0, 1)], "the ball does not turn the pick into the group");

    let before = r.app.scene.groups[0].shapes[1].path.clone();
    let pipe_pos = r.app.scene.groups[0].shapes[1].pos;
    let sibling = r.app.scene.groups[0].shapes[0].pos;
    r.key(egui::Key::ArrowRight);
    let after = &r.app.scene.groups[0].shapes[1].path;
    assert!((after[3][0] - before[3][0] - 1.0).abs() < 1e-4, "Ball 4 moved 1 mm: {:?}", after[3]);
    for i in [0, 1, 2, 4] {
        assert_eq!(after[i], before[i], "node {i} stays");
    }
    assert_eq!(r.app.scene.groups[0].shapes[1].pos, pipe_pos, "the pipe itself did not move");
    assert_eq!(r.app.scene.groups[0].shapes[0].pos, sibling, "the sibling shape did not move");

    // One arrow key is one undo step.
    r.app.undo();
    assert_eq!(r.app.scene.groups[0].shapes[1].path, before, "undo restores the ball");
}

#[test]
fn a_right_click_ends_the_pipe_tool() {
    let mut r = Rig::new();
    r.app.set_pipe_tool(true);
    r.frame(vec![]);
    let p = r.app.view_rect.center() + vec2(-120.0, 60.0);
    r.mv(p);
    r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: true, modifiers: r.mods }]);
    r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: false, modifiers: r.mods }]);
    r.frame(vec![]);
    assert!(!r.app.pt.on, "right click stops the pipe tool");
}

#[test]
fn a_left_drag_on_empty_space_box_selects_by_direction_and_never_turns_the_view() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Select;
    r.app.scene.groups[0].shapes[0].pos = [0.0, 0.0, 20.0];
    r.frame(vec![]);
    let c = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("shape on screen");
    let cam = (r.app.cam.yaw, r.app.cam.pitch);
    // to the left, a box that only touches the shape takes it
    r.drag(c + vec2(30.0, -30.0), c + vec2(-5.0, 10.0));
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), cam, "a left drag never turns the view");
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)), "dragged leftwards: touching is enough");
    r.app.sel = None;
    // to the right, the same small box is not enough: the shape must be wholly inside
    r.drag(c + vec2(-5.0, -30.0), c + vec2(30.0, 10.0));
    assert_eq!(r.app.sel, None, "dragged rightwards: only shapes completely inside");
    // a big box to the right takes it
    r.drag(c + vec2(-260.0, -260.0), c + vec2(260.0, 260.0));
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
}

/// A screen point the viewport draws as one of the axis balls (the positive end of `axis`).
fn gnomon_ball(r: &Rig, axis: usize) -> Pos2 {
    let rect = r.app.view_rect;
    let ball = r
        .app
        .gnomon_items(rect)
        .into_iter()
        .find(|(d, _, positive)| *positive && d[axis] > 0.5)
        .map(|(_, p, _)| p)
        .expect("the positive axis ball is drawn");
    assert!(r.app.gnomon_hit(rect, ball).is_some(), "the test point must hit the axis ball");
    assert!(!App::home_hit(rect, ball), "the axis ball is not the home button");
    ball
}

#[test]
fn a_drag_that_starts_on_the_axis_ball_turns_the_view() {
    let mut r = Rig::new();
    r.app.cam.look(0.6, 0.42);
    r.app.fly = None;
    let ball = gnomon_ball(&r, 0);
    let before = (r.app.cam.yaw, r.app.cam.pitch);
    r.drag(ball, ball + vec2(70.0, 45.0));
    assert_ne!((r.app.cam.yaw, r.app.cam.pitch), before, "dragging the axis ball must turn the view");
    assert!(r.app.fly.is_none(), "a drag is not a click: it must not snap to the axis");
}

#[test]
fn a_shift_drag_that_starts_on_the_axis_ball_slides_the_view() {
    let mut r = Rig::new();
    r.app.cam.look(0.6, 0.42);
    r.app.fly = None;
    let ball = gnomon_ball(&r, 1);
    let before = (r.app.cam.yaw, r.app.cam.pitch);
    let target = r.app.cam.target;
    r.mods = egui::Modifiers::SHIFT;
    r.drag(ball, ball + vec2(60.0, 40.0));
    r.mods = egui::Modifiers::NONE;
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), before, "Shift+drag pans; it does not turn the view");
    assert_ne!(r.app.cam.target, target, "a Shift+drag on the axis ball slides the view");
}

#[test]
fn a_click_on_the_axis_ball_still_snaps_the_view_to_that_axis() {
    let mut r = Rig::new();
    r.app.cam.look(0.6, 0.42);
    r.app.fly = None;
    let ball = gnomon_ball(&r, 0);
    let axis = r.app.gnomon_hit(r.app.view_rect, ball).expect("the click is on an axis");
    let yaw = r.app.cam.yaw;
    r.click(ball);
    assert_eq!(r.app.cam.yaw, yaw, "a click snaps by gliding; it must not orbit");
    let goal = r.app.fly.expect("clicking an axis glides to that view");
    let pitch = axis[2].clamp(-1.0, 1.0).asin();
    let want_yaw = if axis[2].abs() > 0.9 { yaw } else { axis[0].atan2(-axis[1]) };
    assert!((goal.yaw - want_yaw).abs() < 1e-5 && (goal.pitch - pitch).abs() < 1e-5, "goal {:?}", (goal.yaw, goal.pitch));
    for _ in 0..200 {
        r.app.step_fly(0.016);
    }
    assert!(r.app.fly.is_none(), "the glide finishes");
    assert!((r.app.cam.yaw - want_yaw).abs() < 1e-3 && (r.app.cam.pitch - pitch).abs() < 1e-3, "the view looks along the clicked axis");
}

#[test]
fn a_press_on_the_home_button_never_turns_the_view_and_a_click_goes_home() {
    let mut r = Rig::new();
    r.app.cam.look(2.0, -0.4);
    r.app.fly = None;
    let home = App::home_centre(r.app.view_rect);
    assert!(App::home_hit(r.app.view_rect, home));
    let before = (r.app.cam.yaw, r.app.cam.pitch);
    r.drag(home, home + vec2(50.0, 30.0));
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), before, "the home button is not an orbit handle");
    assert!(r.app.fly.is_none(), "a press that turns into a drag does not go home");
    r.click(home);
    let goal = r.app.fly.expect("a click on the home button glides home");
    let first = crate::camera::Camera::default();
    assert_eq!((goal.yaw, goal.pitch), (first.yaw, first.pitch));
}

/// The centre of the dark disc behind the axis balls, as the viewport draws it.
fn gnomon_disc_centre(rect: Rect) -> Pos2 {
    pos2(rect.right() - 62.0, rect.top() + 62.0)
}

/// A point on the dark disc but on no axis ball and not the home button.
fn gnomon_disc_background(r: &Rig) -> Pos2 {
    let rect = r.app.view_rect;
    let c = gnomon_disc_centre(rect);
    for k in 0..24 {
        let a = k as f32 * std::f32::consts::TAU / 24.0;
        let p = c + vec2(a.cos(), a.sin()) * 22.0;
        assert!((p - c).length() < 48.0, "the point is inside the disc");
        if r.app.gnomon_hit(rect, p).is_none() && !App::home_hit(rect, p) {
            return p;
        }
    }
    panic!("the disc has a background between its balls")
}

/// A point just outside the disc, away from every axis ball and the home button.
fn just_outside_gnomon_disc(r: &Rig) -> Pos2 {
    let rect = r.app.view_rect;
    let c = gnomon_disc_centre(rect);
    for k in 0..24 {
        let a = k as f32 * std::f32::consts::TAU / 24.0;
        let p = c + vec2(a.cos(), a.sin()) * 56.0;
        if r.app.gnomon_hit(rect, p).is_none() && !App::home_hit(rect, p) {
            return p;
        }
    }
    panic!("there is room just outside the disc")
}

#[test]
fn a_drag_that_starts_on_the_gnomon_disc_turns_the_view_and_makes_no_box() {
    let mut r = Rig::new();
    r.app.cam.look(0.6, 0.42);
    r.app.fly = None;
    r.app.sel = None;
    let p = gnomon_disc_background(&r);
    let before = (r.app.cam.yaw, r.app.cam.pitch);
    r.mv(p);
    r.button(p, true);
    r.mv(p + vec2(-35.0, 20.0));
    assert!(r.app.marquee_from.is_none(), "a drag on the disc background must not start a box");
    assert_ne!((r.app.cam.yaw, r.app.cam.pitch), before, "a drag on the disc background must turn the view");
    r.mv(p + vec2(-70.0, 40.0));
    r.button(p + vec2(-70.0, 40.0), false);
    r.frame(vec![]);
    r.frame(vec![]);
    assert!(r.app.marquee_from.is_none(), "the box never existed, so none is left behind");
    assert_eq!(r.app.sel, None, "a drag on the disc must not select anything");
    assert!(r.app.fly.is_none(), "a drag is not a click: it must not snap to an axis");
}

#[test]
fn a_click_on_the_gnomon_disc_background_selects_nothing() {
    let mut r = Rig::new();
    let rect = r.app.view_rect;
    let p = gnomon_disc_background(&r);
    // Put a sphere right behind the disc: a click there must not pick it.
    let (o, d) = r.app.cam.ray_at(rect, p);
    r.app.scene.groups[0].shapes[0].pos = [o[0] + d[0] * 120.0, o[1] + d[1] * 120.0, o[2] + d[2] * 120.0];
    r.app.sel = None;
    r.frame(vec![]);
    r.frame(vec![]);
    assert!(r.app.shape_at(rect, p).is_some(), "the test needs a shape behind the disc");
    r.click(p);
    assert_eq!(r.app.sel, None, "a click on the disc background must select nothing behind it");
    assert!(r.app.fly.is_none(), "a click on the background is not an axis snap");
    r.right_click(p);
    assert_eq!(r.app.sel, None, "a right click on the disc picks nothing behind it");
    assert!(!r.ctx.any_popup_open(), "a right click on the disc opens no context menu");
}

#[test]
fn a_press_just_outside_the_gnomon_disc_still_starts_a_box() {
    let mut r = Rig::new();
    r.app.sel = None;
    let p = just_outside_gnomon_disc(&r);
    let before = (r.app.cam.yaw, r.app.cam.pitch);
    r.mv(p);
    r.button(p, true);
    r.mv(p + vec2(-40.0, 30.0));
    assert!(r.app.marquee_from.is_some(), "just outside the disc a box still starts");
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), before, "the box is not an orbit");
    r.button(p + vec2(-40.0, 30.0), false);
    r.frame(vec![]);
}

#[test]
fn one_drag_draws_one_pipe_then_the_tool_ends_and_its_nodes_can_be_moved() {
    let mut r = Rig::new();
    r.app.pt.on = true;
    let rect = r.app.view_rect;
    let c = rect.center();
    r.mv(c + vec2(-150.0, 40.0));
    r.button(c + vec2(-150.0, 40.0), true);
    for k in 1..=24 {
        let t = k as f32 / 24.0;
        r.mv(c + vec2(-150.0 + 300.0 * t, 40.0 - 90.0 * (t * std::f32::consts::PI).sin()));
    }
    r.button(c + vec2(150.0, 40.0), false);
    r.frame(vec![]);
    r.frame(vec![]);
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("selected {:?}", r.app.sel) };
    let count = r.app.scene.shape_count();
    let Some(shape) = r.app.scene.shape(g, s) else { panic!("pipe") };
    let lay = layout(&r.app.cam, r.app.view_rect, shape);
    let mid = shape.path.len() / 2;
    let Some(Some(node)) = lay.nodes.get(mid).copied() else { panic!("node on screen") };
    let before = shape.path.get(mid).copied();
    assert!(!r.app.pt.on, "one drag, one pipe: the tool ended by itself");
    r.drag(node.c, node.c + vec2(0.0, -50.0));
    let after = r.app.scene.shape(g, s).and_then(|s| s.path.get(mid).copied());
    assert_eq!(r.app.scene.shape_count(), count, "no second pipe was traced");
    assert_ne!(before, after, "the new pipe's node moves (before {before:?}, after {after:?})");
}

#[test]
fn several_selected_shapes_grow_together_from_the_middle() {
    for tool in [crate::gizmo::Tool::Scale, crate::gizmo::Tool::All] {
        let mut r = Rig::new();
        r.app.scene.groups[0].shapes[0].pos = [-30.0, 0.0, 20.0];
        r.app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [30.0, 0.0, 20.0], Material::default()));
        r.app.sel = Some(Pick::Shape(0, 0));
        r.app.multi = vec![Pick::Shape(0, 1)];
        r.app.multi_for = r.app.sel;
        r.app.tool = tool;
        r.frame(vec![]);
        let before: Vec<_> = (0..2).filter_map(|i| r.app.scene.shape(0, i).map(|s| (s.pos, s.scale))).collect();
        let centre = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("the middle of both");
        r.drag(centre, centre + vec2(60.0, -60.0));
        let after: Vec<_> = (0..2).filter_map(|i| r.app.scene.shape(0, i).map(|s| (s.pos, s.scale))).collect();
        assert_ne!(before, after, "{tool:?}: dragging the middle handle of several shapes must change them");
        for (b, a) in before.iter().zip(&after) {
            assert!(a.1[0] > b.1[0], "{tool:?}: each shape grows");
            assert!(a.0[0].abs() > b.0[0].abs(), "{tool:?}: and moves apart from the middle, so the group grows as a whole");
        }
    }
}

#[test]
fn pulling_an_arrow_of_several_shapes_out_grows_them_and_in_shrinks_them() {
    let mut r = Rig::new();
    r.app.scene.groups[0].shapes[0].pos = [-30.0, 0.0, 20.0];
    r.app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [30.0, 0.0, 20.0], Material::default()));
    r.app.sel = Some(Pick::Shape(0, 0));
    r.app.multi = vec![Pick::Shape(0, 1)];
    r.app.multi_for = r.app.sel;
    r.app.tool = crate::gizmo::Tool::Scale;
    r.frame(vec![]);
    let rect = r.app.view_rect;
    let c = r.app.cam.project(rect, [0.0, 0.0, 20.0]).expect("the middle");
    let dir = (r.app.cam.project(rect, [10.0, 0.0, 20.0]).expect("along x") - c).normalized();
    let end = c + dir * crate::gizmo::ARROW_PX;
    let at = c + (end - c) * 0.8;
    let out = (end - c).normalized() * 40.0;
    let width = |r: &Rig| r.app.scene.shape(0, 0).map(|s| s.scale[0]).unwrap_or(0.0);
    let w0 = width(&r);
    r.drag(at, at + out);
    assert!(width(&r) > w0, "pulling out grows");
    r.app.undo();
    r.drag(at, at - out * 0.5);
    assert!(width(&r) < w0, "pushing in shrinks");
}

#[test]
fn escape_during_handle_drag_restores_the_shape_and_clears_drag() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Scale;
    let start = r.app.scene.clone();
    let from = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("centre");
    r.mv(from);
    r.button(from, true);
    r.mv(from + vec2(90.0, 60.0));
    assert!(r.app.drag.is_some(), "pointer events started a gizmo drag");
    r.key(egui::Key::Escape);
    assert!(r.app.drag.is_none(), "Escape cancels the active drag");
    assert_eq!(r.app.scene, start, "Escape restores the pre-drag geometry");
    r.button(from + vec2(90.0, 60.0), false);
    r.frame(vec![]);
    assert_eq!(r.app.scene, start, "release after cancellation must not mutate the scene");
}

#[test]
fn a_gizmo_drag_is_exactly_one_undo_step() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Scale;
    let start = r.app.scene.clone();
    let centre = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("centre");
    r.drag(centre, centre + vec2(90.0, -60.0));
    assert_ne!(r.app.scene, start, "scaling changes the shape");
    let end = r.app.scene.clone();
    r.app.undo();
    assert_eq!(r.app.scene, start);
    r.app.redo();
    assert_eq!(r.app.scene, end);
}

#[test]
fn drag_release_outside_viewport_clears_capture_and_keeps_undo() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Scale;
    let start = r.app.scene.clone();
    let centre = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("centre");
    r.mv(centre);
    r.button(centre, true);
    r.mv(pos2(-70.0, -50.0));
    r.button(pos2(-70.0, -50.0), false);
    r.frame(vec![]);
    assert!(r.app.drag.is_none(), "a release outside the window must end a drag");
    if r.app.scene != start {
        r.app.undo();
        assert_eq!(r.app.scene, start);
    }
}

#[test]
fn direct_solid_polygon_clicks_and_enter_complete_without_changing_selection() {
    let mut r = Rig::new();
    r.direct_solid_sketch(Pen::Polygon);
    let selected = r.app.sel;
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        let p = r.on_plane(q);
        r.click(p);
    }
    assert_eq!(r.app.sk.pts.len(), 3, "three pointer clicks make three corners");
    assert_eq!(r.app.sel, selected, "drawing must not pick the solid behind the sketch");
    r.key(egui::Key::Enter);
    assert_eq!(r.app.sk.stage, Stage::Shape);
    assert_eq!(r.app.sk.profile.len(), 3);
    assert!(r.app.sk.pts.is_empty());
}

#[test]
fn direct_solid_sketch_right_click_finishes_line_without_duplicate_endpoint() {
    let mut r = Rig::new();
    r.direct_solid_sketch(Pen::Line);
    r.app.sk.smooth = false;
    let a = r.on_plane([-30.0, -20.0]);
    let b = r.on_plane([25.0, 12.0]);
    r.click(a);
    r.click(b);
    r.right_click(b);
    assert_eq!(r.app.sk.stage, Stage::Draw, "an open line stays in the sketch tool");
    assert_eq!(r.app.sk.path.len(), 2, "left clicks place points, the right click confirms without adding one");
    assert!(r.app.sk.pts.is_empty());
}

#[test]
fn direct_solid_right_click_confirms_a_polygon_and_does_not_open_the_menu_or_pick() {
    let mut r = Rig::new();
    r.direct_solid_sketch(Pen::Polygon);
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        let p = r.on_plane(q);
        r.click(p);
    }
    let p = r.on_plane([24.0, 18.0]);
    r.right_click(p);
    assert_eq!(r.app.sk.stage, Stage::Shape);
    assert_eq!(r.app.sk.profile.len(), 3);
    let live = r.app.sk.live.map(|(g, s)| Pick::Shape(g, s));
    assert!(live.is_some());
    assert_eq!(r.app.sel, live, "the right click closed the outline; it must not pick the solid behind");
}

#[test]
fn sketch_rectangle_and_circle_drag_are_profiles_not_camera_orbits() {
    for (pen, sides) in [(Pen::Rect, 4), (Pen::Circle, sketch::CIRCLE_POINTS)] {
        let mut r = Rig::new();
        r.sketch(pen);
        let yaw = r.app.cam.yaw;
        let pitch = r.app.cam.pitch;
        let a = r.on_plane([-25.0, -14.0]);
        let b = r.on_plane([25.0, 17.0]);
        r.drag(a, b);
        assert_eq!(r.app.sk.stage, Stage::Ready, "{pen:?} drag must finish the draft without extruding it");
        assert!(r.app.sk.live.is_none());
        assert_eq!(r.app.sk.profile.len(), sides);
        assert!(!r.app.sk.dragging && r.app.sk.anchor.is_none());
        assert_eq!((r.app.cam.yaw, r.app.cam.pitch), (yaw, pitch), "sketch drag must not orbit");
    }
}

#[test]
fn delete_without_curve_node_selection_still_deletes_selected_shape() {
    let mut r = Rig::new();
    let n = r.app.scene.shape_count();
    r.key(egui::Key::Delete);
    assert_eq!(r.app.scene.shape_count(), n - 1, "Delete regression");
}

#[test]
fn direct_solid_rectangle_enter_finishes_a_two_click_preview() {
    let mut r = Rig::new();
    r.direct_solid_sketch(Pen::Rect);
    let a = r.on_plane([-25.0, -15.0]);
    let b = r.on_plane([21.0, 18.0]);
    r.click(a);
    assert!(r.app.sk.anchor.is_some());
    r.mv(b);
    r.key(egui::Key::Enter);
    assert_eq!(r.app.sk.stage, Stage::Shape);
    assert_eq!(r.app.sk.profile.len(), 4);
    assert!(r.app.sk.anchor.is_none());
}

#[test]
fn sketch_escape_cancels_pending_points_before_ending_tool() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let p = r.on_plane([-20.0, -10.0]);
    r.click(p);
    assert_eq!(r.app.sk.pts.len(), 1);
    r.key(egui::Key::Escape);
    assert!(r.app.sk.on);
    assert!(r.app.sk.pts.is_empty());
    r.key(egui::Key::Escape);
    assert!(!r.app.sk.on);
}

#[test]
fn sketch_unfinished_polygon_drag_cannot_start_camera_orbit() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let yaw = r.app.cam.yaw;
    let pitch = r.app.cam.pitch;
    let a = r.on_plane([-28.0, -24.0]);
    let b = r.on_plane([24.0, 22.0]);
    r.drag(a, b);
    assert_eq!((r.app.cam.yaw, r.app.cam.pitch), (yaw, pitch));
    assert!(r.app.sk.pts.is_empty(), "a drag is not a point click");
    let p = r.on_plane([12.0, -14.0]);
    r.click(p);
    assert_eq!(r.app.sk.pts.len(), 1, "the next click must still draw");
}

#[test]
fn sketch_drag_lost_outside_window_cancels_without_stale_anchor() {
    let mut r = Rig::new();
    r.sketch(Pen::Rect);
    let start = r.on_plane([-25.0, -20.0]);
    r.mv(start);
    r.button(start, true);
    r.mv(start + vec2(35.0, -15.0));
    assert!(r.app.sk.dragging);
    r.frame(vec![Event::PointerGone]);
    // egui may keep the button held while the pointer is outside; the release then ends the gesture
    r.button(pos2(-50.0, -50.0), false);
    r.frame(vec![]);
    assert!(!r.app.sk.dragging);
    assert!(r.app.sk.anchor.is_none(), "no phantom first corner after the pointer was lost");
    assert_eq!(r.app.sk.stage, Stage::Draw);
    let next = r.on_plane([15.0, -14.0]);
    r.click(next);
    assert!(r.app.sk.anchor.is_some(), "a fresh two-click shape can start");
}

#[test]
fn sketch_drag_release_outside_viewport_discards_partial_shape() {
    let mut r = Rig::new();
    r.sketch(Pen::Circle);
    let start = r.on_plane([-25.0, -20.0]);
    r.mv(start);
    r.button(start, true);
    r.mv(start + vec2(30.0, -20.0));
    assert!(r.app.sk.dragging);
    r.button(pos2(-30.0, -30.0), false);
    assert!(!r.app.sk.dragging);
    assert!(r.app.sk.anchor.is_none());
    assert_eq!(r.app.sk.stage, Stage::Draw);
    let next = r.on_plane([10.0, 5.0]);
    r.click(next);
    assert!(r.app.sk.anchor.is_some());
}

#[test]
fn sketch_card_consumes_pointer_input_without_drawing_beneath_it() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let selected = r.app.sel;
    // the very top edge of the card: padding only, so no button under it can fire
    let top = r.ctx.memory(|m| m.area_rect(egui::Id::new("sketch_card"))).map_or(r.app.view_rect.top() + 12.0, |a| a.top());
    let card = pos2(r.app.view_rect.center().x, top + 3.0);
    r.click(card);
    assert!(r.app.sk.pts.is_empty(), "card header must not place a sketch point");
    assert_eq!(r.app.sel, selected);
    let canvas = r.on_plane([20.0, -15.0]);
    r.click(canvas);
    assert_eq!(r.app.sk.pts.len(), 1, "outside the card, the viewport still accepts clicks");
}

#[test]
fn sketch_pointer_coordinates_remain_stable_after_resize_and_high_dpi() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    for (size, scale) in [(vec2(1100.0, 760.0), 1.0), (vec2(900.0, 620.0), 2.0), (vec2(1450.0, 940.0), 1.5)] {
        r.size = size;
        r.ctx.set_pixels_per_point(scale);
        r.app.sk.clear_drawing();
        r.frame(vec![]);
        r.frame(vec![]);
        let p = r.on_plane([17.0, -14.0]);
        r.click(p);
        assert_eq!(r.app.sk.pts.as_slice(), &[[17.0, -14.0]], "logical point coordinates at {scale} ppp, {size:?}");
    }
}

impl Rig {
    /// The sketch tool just started: looking down, no plane chosen yet.
    fn sketch_unpicked(&mut self, pen: Pen) {
        self.app.set_sketch_tool(true);
        self.app.sk.pen = pen;
        self.app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        self.app.cam.target = [0.0, 0.0, 0.0];
        self.app.cam.dist = 140.0;
        self.app.fly = None;
        self.frame(vec![]);
        self.frame(vec![]);
    }
}

#[test]
fn one_click_on_the_ground_picks_the_floor_and_places_the_first_point() {
    let mut r = Rig::new();
    r.sketch_unpicked(Pen::Polygon);
    assert!(r.app.picking_now(), "the viewport waits for a plane");
    let p = r.on_plane([55.0, 45.0]);
    r.mv(p);
    assert!(r.app.sk.hover_frame.is_some_and(|(_, face, _)| !face), "hovering empty ground shows the floor");
    r.click(p);
    assert!(!r.app.picking_now());
    assert_eq!(r.app.sk.frame, Frame::floor());
    assert_eq!(r.app.sk.pts.len(), 1, "the same click placed the first point");
    assert_eq!(r.app.sk.pts.first().copied(), Some([55.0, 45.0]));
}

#[test]
fn one_click_on_a_face_picks_that_face_and_places_the_first_point() {
    let mut r = Rig::new();
    r.sketch_unpicked(Pen::Polygon);
    let p = r.on_plane([3.0, 2.0]);
    r.mv(p);
    assert!(r.app.sk.hover_frame.is_some_and(|(_, face, _)| face), "hovering the sphere shows a square on its surface");
    r.click(p);
    assert!(!r.app.picking_now());
    assert_eq!(r.app.sk.plane, crate::sketch_ui::PlaneSel::Custom);
    assert!(r.app.sk.frame.origin[2] > 10.0, "the plane sits on the sphere, not on the floor");
    assert!(r.app.sk.frame.normal()[2] > 0.9, "the top of the sphere faces up");
    assert_eq!(r.app.sk.pts.len(), 1);
    assert!(r.app.fly.is_some(), "the camera glides to face the plane");
}

#[test]
fn a_rectangle_drag_on_the_ground_picks_the_plane_too() {
    let mut r = Rig::new();
    r.sketch_unpicked(Pen::Rect);
    let a = r.on_plane([50.0, 40.0]);
    let b = r.on_plane([62.0, 50.0]);
    r.drag(a, b);
    assert!(!r.app.picking_now());
    assert_eq!(r.app.sk.stage, Stage::Ready, "the rectangle is a draft until the sketch is confirmed");
    assert!(r.app.sk.live.is_none());
    assert_eq!(r.app.sk.profile.len(), 4);
}

#[test]
fn re_pick_plane_waits_for_a_click_again() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    assert!(!r.app.picking_now());
    r.app.sketch_repick();
    assert!(r.app.picking_now());
    r.app.sketch_set_plane(crate::sketch_ui::PlaneSel::Side);
    assert!(!r.app.picking_now());
}

#[test]
fn sketch_alt_primary_drag_still_controls_the_camera() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let yaw = r.app.cam.yaw;
    let pitch = r.app.cam.pitch;
    r.mods = egui::Modifiers::ALT;
    let a = r.on_plane([-30.0, -25.0]);
    let b = r.on_plane([25.0, 25.0]);
    r.drag(a, b);
    r.mods = egui::Modifiers::NONE;
    assert_ne!((r.app.cam.yaw, r.app.cam.pitch), (yaw, pitch), "Alt+drag must still orbit");
    assert!(r.app.sk.pts.is_empty(), "orbit must not place sketch points");
}

#[test]
fn ctrl_click_selects_multiple_shapes_and_right_click_keeps_the_selection() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Select;
    r.app.scene.groups[0].shapes[0].pos = [-35.0, 0.0, 20.0];
    r.app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [45.0, 0.0, 20.0], Material::default()));
    r.frame(vec![]);
    let p0 = r.app.cam.project(r.app.view_rect, [-35.0, 0.0, 20.0]).expect("first shape visible");
    let p1 = r.app.cam.project(r.app.view_rect, [45.0, 0.0, 20.0]).expect("second shape visible");
    assert_eq!(r.app.shape_at(r.app.view_rect, p0), Some((0, 0)));
    assert_eq!(r.app.shape_at(r.app.view_rect, p1), Some((0, 1)));

    r.mods = egui::Modifiers::NONE;
    r.click(p0);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
    r.mods = egui::Modifiers::CTRL;
    r.click(p1);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
    assert!(r.app.is_pick_selected(Pick::Shape(0, 1)));
    assert_eq!(r.app.multi.len(), 1);

    r.click(p1);
    assert_eq!(r.app.multi.len(), 0, "Ctrl-click toggles an additional pick off");
    r.mods = egui::Modifiers::CTRL;
    r.click(p1);
    assert_eq!(r.app.multi.len(), 1);
    r.click(p0);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 1)), "Ctrl-clicking the primary promotes the next pick");
    assert!(r.app.multi.is_empty());
    r.click(p1);
    assert_eq!(r.app.sel, None, "Ctrl-clicking the sole primary clears the selection");

    // Keep the current Shift-click toggle behavior from the integration base.
    r.mods = egui::Modifiers::SHIFT;
    r.click(p0);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
    r.click(p1);
    assert!(r.app.is_pick_selected(Pick::Shape(0, 1)));
    r.click(p1);
    assert!(!r.app.is_pick_selected(Pick::Shape(0, 1)));

    r.mods = egui::Modifiers::CTRL;
    r.click(p1);
    assert_eq!(r.app.multi.len(), 1);
    r.frame(vec![Event::PointerButton { pos: p1, button: PointerButton::Secondary, pressed: true, modifiers: r.mods }]);
    r.frame(vec![Event::PointerButton { pos: p1, button: PointerButton::Secondary, pressed: false, modifiers: r.mods }]);
    assert_eq!(r.app.multi.len(), 1, "right-clicking a selected shape preserves the multi-selection");
}

#[test]
fn solid_sketch_polygon_right_click_saves_only_the_sketch_and_returns_to_selection() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Polygon);
    let before = r.app.scene.clone();
    let undo_before = r.app.undo_depth();
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        r.click(r.on_plane(q));
    }
    assert_eq!(r.app.scene, before, "placing corners must not create a solid or a saved sketch");
    r.right_click(r.on_plane([24.0, 18.0]));
    assert!(!r.app.sketch_active(), "right click finishes the sketch and returns to selection");
    assert!(r.app.sk.live.is_none(), "sketch confirmation must not begin an extrusion");
    assert_eq!(r.app.scene.shape_count(), before.shape_count() + 1);
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the saved sketch is selected") };
    let guide = r.app.scene.shape(g, s).expect("saved sketch");
    assert!(guide.guide && guide.kind == Kind::Curve && !guide.solid());
    assert!(crate::curve::settings(&guide.params).0, "the polygon is closed");
    assert!(!egui::Popup::is_any_open(&r.ctx), "the confirming right click must not open the viewport menu");
    assert_eq!(r.app.undo_depth(), undo_before + 1, "the entire sketch is one undo step");
    let saved = r.app.scene.clone();
    r.app.undo();
    r.frame(vec![]);
    assert_eq!(r.app.scene, before);
    r.app.redo();
    r.frame(vec![]);
    assert_eq!(r.app.scene, saved, "redo restores the same sketch without starting a drawing tool");
}

#[test]
fn solid_sketch_rectangle_and_circle_wait_for_right_click_after_two_left_clicks() {
    for (pen, points) in [(Pen::Rect, 4), (Pen::Circle, sketch::CIRCLE_POINTS)] {
        let mut r = Rig::new();
        r.plain_sketch(pen);
        let before = r.app.scene.clone();
        r.click(r.on_plane([-25.0, -15.0]));
        r.click(r.on_plane([5.0, 5.0]));
        assert_eq!(r.app.sk.stage, Stage::Ready, "{pen:?}: the second left click defines the draft");
        assert_eq!(r.app.sk.profile.len(), points);
        assert_eq!(r.app.scene, before, "{pen:?}: dimensions are still editable before confirmation");
        assert!(r.app.sk.live.is_none());
        assert!(r.app.sk.dimension.as_ref().is_some_and(|d| d.editing));
        r.right_click(r.on_plane([5.0, 5.0]));
        assert_eq!(r.app.scene.shape_count(), before.shape_count() + 1);
        let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the saved sketch is selected") };
        let guide = r.app.scene.shape(g, s).expect("saved sketch");
        assert!(guide.guide && !guide.solid(), "{pen:?}: confirmation saves a guide, not a solid");
        assert!(!r.app.sketch_active());
        assert!(!egui::Popup::is_any_open(&r.ctx));
    }
}

#[test]
fn solid_sketch_open_line_is_saved_without_an_extra_endpoint() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Line);
    r.app.sk.smooth = false;
    let before = r.app.scene.shape_count();
    r.click(r.on_plane([-30.0, -20.0]));
    r.click(r.on_plane([25.0, 12.0]));
    r.right_click(r.on_plane([35.0, 22.0]));
    assert_eq!(r.app.scene.shape_count(), before + 1);
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the saved line is selected") };
    let guide = r.app.scene.shape(g, s).expect("saved line");
    assert!(guide.guide && guide.kind == Kind::Curve && !guide.solid());
    assert!(!crate::curve::settings(&guide.params).0, "the line must remain open");
    assert_eq!(guide.path.len(), 2, "right click confirms the two left-clicked points without adding a third");
    let world: Vec<_> = guide.path.iter().map(|p| crate::curve_tool::to_world(guide, [p[0], p[1], p[2]])).collect();
    assert_eq!(world, [[-30.0, -20.0, 0.0], [25.0, 12.0, 0.0]]);
    assert!(!r.app.sketch_active());
    assert!(!egui::Popup::is_any_open(&r.ctx));
}

#[test]
fn solid_sketch_dimension_enter_changes_the_draft_and_right_click_saves_exact_dimensions() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Rect);
    let before = r.app.scene.clone();
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    r.replace_text("100");
    r.key(egui::Key::Tab);
    r.replace_text("50");
    r.key(egui::Key::Enter);
    assert_eq!(r.app.scene, before, "Enter accepts dimensions without saving a sketch or creating a solid");
    assert_eq!(r.app.sk.stage, Stage::Ready);
    assert_eq!(r.app.sk.profile, sketch::rect([-25.0, -15.0], [75.0, 35.0]));
    r.right_click(r.on_plane([5.0, 5.0]));
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the saved sketch is selected") };
    let (frame, outline) = r.app.curve_profile(g, s).expect("saved sketch keeps its exact outline");
    let world: Vec<_> = outline.iter().map(|p| frame.to_world(*p)).collect();
    assert_eq!(world, [[-25.0, -15.0, 0.0], [75.0, -15.0, 0.0], [75.0, 35.0, 0.0], [-25.0, 35.0, 0.0]]);
    assert!(r.app.sk.live.is_none());
}

#[test]
fn solid_sketch_invalid_dimension_blocks_right_click_and_escape_restores_the_draft() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Rect);
    let before = r.app.scene.clone();
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    let original = r.app.sk.profile.clone();
    r.replace_text("NaN");
    assert!(r.app.sk.dimension.as_ref().is_some_and(|d| d.error.is_some()));
    r.right_click(r.on_plane([5.0, 5.0]));
    assert_eq!(r.app.scene, before, "invalid text must not save the last valid geometry silently");
    assert!(r.app.sketch_active());
    assert_eq!(r.app.sk.stage, Stage::Ready);
    r.key(egui::Key::Escape);
    assert_eq!(r.app.sk.profile, original, "Escape cancels only the dimension edit");
    assert_eq!(r.app.sk.stage, Stage::Ready);
    r.right_click(r.on_plane([5.0, 5.0]));
    assert_eq!(r.app.scene.shape_count(), before.shape_count() + 1);
    assert!(!r.app.sketch_active());
}

#[test]
fn solid_sketch_and_extrusion_are_separate_right_click_confirmations_and_undo_steps() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Rect);
    let before = r.app.scene.clone();
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    r.right_click(r.on_plane([5.0, 5.0]));
    let sketch_scene = r.app.scene.clone();
    let guide_pick = r.app.sel;
    let undo_before_extrude = r.app.undo_depth();
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    r.frame(vec![]);
    let (g, s) = r.app.sk.live.expect("Extrude starts a separate solid preview");
    let shape = r.app.scene.shape(g, s).expect("preview shape");
    assert_eq!(shape.kind, Kind::Extrude);
    assert_eq!(shape.outline, sketch::rect([-15.0, -10.0], [15.0, 10.0]));
    assert_eq!(shape.pos, [-10.0, -5.0, r.app.sk.height * 0.5]);
    assert_eq!(r.app.scene.shape_count(), sketch_scene.shape_count() + 1);
    assert_eq!(r.app.undo_depth(), undo_before_extrude, "a live preview is not an accepted operation");
    r.click(r.on_plane([35.0, -25.0]));
    assert_eq!(r.app.sk.live, Some((g, s)), "left click must not confirm the extrusion");
    r.right_click(r.on_plane([35.0, -25.0]));
    assert!(r.app.sk.live.is_none());
    assert!(!r.app.sketch_active(), "solid confirmation returns to selection");
    assert_eq!(r.app.sel, Some(Pick::Shape(g, s)));
    assert!(!egui::Popup::is_any_open(&r.ctx), "the confirming right click must not open the viewport menu");
    assert_eq!(r.app.undo_depth(), undo_before_extrude + 1);
    if let Some(Pick::Shape(gg, gs)) = guide_pick {
        assert!(r.app.scene.shape(gg, gs).is_some_and(|sh| sh.guide), "the source sketch remains editable");
    } else {
        panic!("source sketch selection");
    }
    r.app.undo();
    r.frame(vec![]);
    assert_eq!(r.app.scene, sketch_scene, "first undo removes only the extrusion");
    r.app.undo();
    r.frame(vec![]);
    assert_eq!(r.app.scene, before, "second undo removes the earlier sketch");
}

#[test]
fn solid_sketch_extrusion_escape_keeps_the_saved_guide_and_adds_no_undo_step() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Polygon);
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        r.click(r.on_plane(q));
    }
    r.right_click(r.on_plane([24.0, 18.0]));
    let sketch_scene = r.app.scene.clone();
    let guide_pick = r.app.sel;
    let undo_before = r.app.undo_depth();
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    r.frame(vec![]);
    assert!(r.app.sk.live.is_some());
    r.key(egui::Key::Escape);
    assert_eq!(r.app.scene, sketch_scene, "Escape discards only the unfinished solid");
    assert_eq!(r.app.sel, guide_pick);
    assert!(!r.app.sketch_active());
    assert!(r.app.sk.live.is_none());
    assert_eq!(r.app.undo_depth(), undo_before);
}

#[test]
fn solid_sketch_typed_rectangle_survives_scene_roundtrip_then_extrudes_at_the_same_size() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Rect);
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    r.replace_text("100");
    r.key(egui::Key::Tab);
    r.replace_text("50");
    r.key(egui::Key::Enter);
    r.right_click(r.on_plane([5.0, 5.0]));
    let data = serde_json::to_string(&r.app.scene).expect("serialize the saved sketch");
    let restored: crate::model::Scene = serde_json::from_str(&data).expect("reload the saved sketch");
    assert_eq!(restored, r.app.scene);
    let mut loaded = App::new(true, None);
    loaded.scene = restored;
    loaded.sel = r.app.sel;
    assert!(loaded.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    let (g, s) = loaded.sk.live.expect("the reloaded sketch can be extruded");
    let shape = loaded.scene.shape(g, s).expect("extrusion after reload");
    assert_eq!(shape.kind, Kind::Extrude);
    assert_eq!(shape.outline, sketch::rect([-50.0, -25.0], [50.0, 25.0]), "use the accepted dimensions, not the initial mouse dimensions");
    assert_eq!(shape.pos, [25.0, 10.0, loaded.sk.height * 0.5]);
}

#[test]
fn solid_sketch_switching_to_pipe_discards_the_unconfirmed_extrusion() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Polygon);
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        r.click(r.on_plane(q));
    }
    r.right_click(r.on_plane([24.0, 18.0]));
    let sketch_scene = r.app.scene.clone();
    let guide_pick = r.app.sel;
    let undo_before = r.app.undo_depth();
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    r.frame(vec![]);
    assert!(r.app.sk.live.is_some());
    // The normal pipe activation path calls sketch_off; it must cancel the preview.
    r.app.set_pipe_tool(true);
    r.frame(vec![]);
    r.frame(vec![]);
    assert!(r.app.pt.on);
    assert!(!r.app.sketch_active());
    assert!(r.app.sk.live.is_none());
    assert_eq!(r.app.sel, guide_pick);
    assert_eq!(r.app.scene, sketch_scene, "switching tools must not silently keep the unconfirmed solid");
    assert_eq!(r.app.undo_depth(), undo_before);
}

#[test]
fn solid_sketch_circle_diameter_input_is_preserved_by_confirmation_and_extrusion() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Circle);
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([-5.0, -15.0]));
    r.key(egui::Key::Tab);
    r.replace_text("50");
    r.key(egui::Key::Enter);
    assert!(r.app.sk.dimension.as_ref().is_some_and(|d| d.fields == ["25".to_string(), "50".to_string()]));
    r.right_click(r.on_plane([-5.0, -15.0]));
    let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("the saved circle is selected") };
    let (frame, outline) = r.app.curve_profile(g, s).expect("saved circle profile");
    assert_eq!(outline.len(), sketch::CIRCLE_POINTS, "saving must preserve all circle samples");
    for point in outline {
        let world = frame.to_world(point);
        assert!(((world[0] + 25.0).hypot(world[1] + 15.0) - 25.0).abs() < 1e-4, "circle point {world:?}");
        assert!(world[2].abs() < 1e-5);
    }
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    let (g, s) = r.app.sk.live.expect("circle extrusion preview");
    let shape = r.app.scene.shape(g, s).expect("circle extrusion");
    assert_eq!(shape.kind, Kind::Extrude);
    assert_eq!(shape.outline.len(), sketch::CIRCLE_POINTS);
    for point in &shape.outline {
        assert!((point[0].hypot(point[1]) - 25.0).abs() < 1e-4, "extrusion point {point:?}");
    }
    assert_eq!(shape.pos, [-25.0, -15.0, r.app.sk.height * 0.5]);
}

#[test]
fn solid_sketch_right_drag_does_not_confirm_the_ready_draft() {
    let mut r = Rig::new();
    r.plain_sketch(Pen::Rect);
    let before = r.app.scene.clone();
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    let start = r.on_plane([35.0, -25.0]);
    r.mv(start);
    r.frame(vec![Event::PointerButton { pos: start, button: PointerButton::Secondary, pressed: true, modifiers: r.mods }]);
    let end = start + vec2(45.0, 20.0);
    for step in 1..=5 {
        r.mv(start + (end - start) * (step as f32 / 5.0));
    }
    r.frame(vec![Event::PointerButton { pos: end, button: PointerButton::Secondary, pressed: false, modifiers: r.mods }]);
    r.frame(vec![]);
    assert_eq!(r.app.scene, before, "camera navigation with the right button must not finish a sketch");
    assert_eq!(r.app.sk.stage, Stage::Ready);
    assert!(r.app.sketch_active());
    assert!(!egui::Popup::is_any_open(&r.ctx));
}

#[test]
fn solid_sketch_ready_delete_does_not_delete_the_previously_selected_body() {
    let mut r = Rig::new();
    r.app.sel = Some(Pick::Shape(0, 0));
    r.plain_sketch(Pen::Rect);
    let before = r.app.scene.clone();
    r.click(r.on_plane([-25.0, -15.0]));
    r.click(r.on_plane([5.0, 5.0]));
    assert_eq!(r.app.sk.stage, Stage::Ready);
    // Test the command's Delete path after leaving the dimension field, rather than
    // merely deleting a character from the focused TextEdit.
    r.app.sk.dimension = None;
    r.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    r.key(egui::Key::Delete);
    assert_eq!(r.app.scene, before, "Delete belongs to the ready draft, not the body's old selection");
    assert!(r.app.sk.live.is_none());
}

#[test]
fn solid_sketch_scene_drag_cannot_reindex_a_preview_and_works_again_after_cancel() {
    let mut r = Rig::new();
    let mut body = Shape::new(Kind::Extrude, [0.0, 0.0, 10.0], Material::default());
    body.name = "Existing extrusion".into();
    r.app.scene.groups[0].shapes[0] = body;
    let source = r.saved_rectangle();
    let saved = r.app.scene.clone();
    r.app.solid_mode = true;
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    let live = r.app.sk.live.expect("extrusion preview");
    assert_eq!(live, (0, 2));
    let frozen = r.app.scene.clone();

    // Drive the same outliner rows a person drags, including the real panels and
    // floating command card. Moving body 0 after preview 2 used to leave sk.live
    // pointing at the existing extrusion, which Escape would then delete.
    r.full_ui = true;
    r.frame(vec![]);
    r.frame(vec![]);
    let body_row = r.row_rect(Pick::Shape(0, 0));
    let preview_row = r.row_rect(Pick::Shape(live.0, live.1));
    r.drag(pos2(body_row.left() + 75.0, body_row.center().y), pos2(preview_row.left() + 75.0, preview_row.bottom() - 3.0));
    assert_eq!(r.app.scene, frozen, "dragging a scene row cannot reorder an active command's references");
    assert_eq!(r.app.sk.live, Some(live));
    assert_eq!(r.app.sk.source_sketch, Some(source));
    r.app.sk.height = 61.0;
    r.app.live_update();
    assert_eq!(r.app.scene.shape(0, 0), saved.shape(0, 0), "changing preview height must leave the earlier extrusion intact");
    assert_eq!(r.app.scene.shape(source.0, source.1), saved.shape(source.0, source.1));
    r.key(egui::Key::Escape);
    assert_eq!(r.app.scene, saved, "Escape removes only the preview");
    assert!(!r.app.scene_structure_locked());

    // A positive control proves that the pointer events reached draggable rows.
    // The legacy in-place outline editor also safely ends before reordering;
    // its already committed shape remains editable through ordinary commands.
    r.app.sketch_edit_begin(0, 0);
    assert!(r.app.sk.edit.is_some());
    r.frame(vec![]);
    r.frame(vec![]);
    let body_row = r.row_rect(Pick::Shape(0, 0));
    let guide_row = r.row_rect(Pick::Shape(source.0, source.1));
    r.drag(pos2(body_row.left() + 75.0, body_row.center().y), pos2(guide_row.left() + 75.0, guide_row.bottom() - 3.0));
    assert!(r.app.sk.edit.is_none(), "reordering ends the old outline editor before its target index changes");
    assert_eq!(r.app.scene.shape(0, 0), saved.shape(source.0, source.1), "the guide moves before the original after the command ends");
    assert_eq!(r.app.scene.shape(0, 1), saved.shape(0, 0));
}

#[test]
fn solid_sketch_scene_actions_preserve_live_and_saved_edit_references() {
    type Action = fn(&mut App, &egui::Context);
    let actions: [(&str, Action); 10] = [
        ("move layer", |app, _| app.move_layer(Pick::Shape(0, 0), Pick::Shape(0, 1), true)),
        ("duplicate", |app, _| app.duplicate()),
        ("mirror copy", |app, _| app.mirror_copy()),
        ("delete", |app, _| app.delete_selected()),
        ("group", |app, _| app.group_selected()),
        ("add group", |app, _| app.add_group()),
        ("add shape", |app, ctx| app.add_shape(ctx, Kind::Box)),
        ("linked copy", |app, ctx| app.make_instance(ctx)),
        ("pattern", |app, ctx| app.set_pattern_kind(ctx, crate::pattern::PatternKind::Rect)),
        ("paste", |app, _| {
            let text = crate::clip::copy_text(app).expect("selected source can be copied");
            assert!(crate::clip::paste_text(app, &text).is_err());
        }),
    ];
    for edit_saved in [false, true] {
        let mut r = Rig::new();
        let source = r.saved_rectangle();
        let saved = r.app.scene.clone();
        if edit_saved {
            assert!(r.app.edit_saved_sketch(source.0, source.1));
        } else {
            assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
        }
        let scene = r.app.scene.clone();
        let live = r.app.sk.live;
        for (label, action) in actions {
            r.app.sel = Some(Pick::Shape(0, 0));
            r.app.multi = vec![Pick::Shape(source.0, source.1)];
            r.app.multi_for = r.app.sel;
            action(&mut r.app, &r.ctx);
            assert_eq!(r.app.scene, scene, "{label} during saved edit={edit_saved} must keep all scene identities");
            assert_eq!(r.app.sk.live, live);
            if edit_saved {
                assert_eq!(r.app.sk.guide_edit, Some(source));
            } else {
                assert_eq!(r.app.sk.source_sketch, Some(source));
            }
        }
        if edit_saved {
            r.app.undo();
            assert_eq!(r.app.scene, saved, "Undo must not replace the scene under a saved sketch edit");
        }
        r.app.sketch_off();
        assert_eq!(r.app.scene, saved);
        r.app.sel = Some(Pick::Shape(0, 0));
        r.app.multi.clear();
        r.app.duplicate();
        assert_eq!(r.app.scene.shape_count(), saved.shape_count() + 1, "scene editing resumes after leaving the command");
    }
}

#[test]
fn solid_sketch_tab_keeps_cut_through_preview_updates_and_confirmation() {
    let mut r = Rig::new();
    r.saved_rectangle();
    r.app.new_op = crate::model::Op::Add;
    assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude));
    let live = r.app.sk.live.expect("extrusion preview");
    assert_eq!(r.app.scene.shape(live.0, live.1).expect("preview").op, crate::model::Op::Add);
    r.key(egui::Key::Tab);
    assert_eq!(r.app.new_op, crate::model::Op::Subtract);
    assert_eq!(r.app.scene.shape(live.0, live.1).expect("preview").op, crate::model::Op::Subtract);
    r.app.sk.height = 61.0;
    r.app.live_update();
    assert_eq!(r.app.scene.shape(live.0, live.1).expect("resized preview").op, crate::model::Op::Subtract);
    r.right_click(r.on_plane([35.0, -25.0]));
    assert!(!r.app.sketch_active());
    assert_eq!(r.app.scene.shape(live.0, live.1).expect("finished solid").op, crate::model::Op::Subtract);
    assert_eq!(r.app.scene.shape(0, 0).expect("original body").op, crate::model::Op::Add, "Tab controls the new solid only");
}

#[test]
fn solid_outline_undo_of_scene_reordering_does_not_rebind_the_editor() {
    let mut r = Rig::new();
    let mut original = Shape::new(Kind::Extrude, [0.0, 0.0, 10.0], Material::default());
    original.name = "Original extrusion".into();
    r.app.scene.groups[0].shapes[0] = original;
    r.app.add_shape(&r.ctx, Kind::Extrude);
    r.frame(vec![]);
    let before = r.app.scene.clone();
    r.app.move_layer(Pick::Shape(0, 0), Pick::Shape(0, 1), true);
    r.frame(vec![]);
    let after = r.app.scene.clone();
    assert_ne!(before, after);
    r.app.sketch_edit_begin(0, 0);
    assert!(r.app.sk.edit.is_some());
    r.app.undo();
    assert_eq!(r.app.scene, before);
    assert!(r.app.sk.edit.is_none(), "two same-kind shapes at the same index are not the same history target");
    r.app.redo();
    assert_eq!(r.app.scene, after);
    assert!(r.app.sk.edit.is_none(), "redo must not resurrect a stale editor reference");
}

#[test]
fn control_preserves_live_sketch_drafts_and_outline_edits() {
    use serde_json::json;

    for mode in ["live", "saved sketch", "ready draft", "outline"] {
        let mut r = Rig::new();
        let source = r.saved_rectangle();
        let saved = r.app.scene.clone();
        match mode {
            "live" => assert!(r.app.make_from_selected_line(&r.ctx, crate::sketch_ui::Make::Extrude)),
            "saved sketch" => assert!(r.app.edit_saved_sketch(source.0, source.1)),
            "ready draft" => {
                r.sketch(Pen::Rect);
                r.click(r.on_plane([-20.0, -15.0]));
                r.click(r.on_plane([20.0, 15.0]));
                assert_eq!(r.app.sk.stage, Stage::Ready);
            }
            "outline" => {
                r.app.add_shape(&r.ctx, Kind::Extrude);
                r.frame(vec![]);
                let Some(Pick::Shape(g, s)) = r.app.sel else { panic!("new extrusion") };
                r.app.sketch_edit_begin(g, s);
                assert!(r.app.sk.edit.is_some());
            }
            _ => unreachable!(),
        }
        r.frame(vec![]);
        let before = r.app.scene.clone();
        let (undo, dirty) = (r.app.undo_depth(), r.app.dirty);
        let refs = (r.app.sk.live, r.app.sk.source_sketch, r.app.sk.guide_edit, r.app.sk.edit.as_ref().map(|e| (e.g, e.s)));
        let (profile, points) = (r.app.sk.profile.clone(), r.app.sk.pts.clone());
        let dir = std::env::temp_dir().join(format!("meltcad-control-edit-{}-{mode}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test folder");
        let load = dir.join("input.meltcad");
        let save = dir.join("draft.meltcad");
        let render = dir.join("view.png");
        std::fs::write(&load, saved.try_to_json().expect("saved scene JSON")).expect("test scene");
        let _ = std::fs::remove_file(&save);
        let commands = [
            json!({"cmd": "commands"}),
            json!({"cmd": "scene.get"}),
            json!({"cmd": "scene.summary"}),
            json!({"cmd": "render", "path": render, "size": 40}),
            json!({"cmd": "scene.new"}),
            json!({"cmd": "scene.set", "scene": crate::model::Scene::default()}),
            json!({"cmd": "load", "path": load}),
            json!({"cmd": "shape.add", "kind": "Box"}),
            json!({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [4, 5, 6]}),
            json!({"cmd": "shape.delete", "group": 0, "shape": 0}),
            json!({"cmd": "group.delete", "group": 0}),
            json!({"cmd": "melt.set", "amount": 12}),
            json!({"cmd": "save", "path": save}),
        ];
        let replies = r.control(&commands);
        for (command, reply) in commands.iter().zip(&replies).take(4) {
            assert!(reply.is_ok(), "{mode}: {command}: {reply:?}");
        }
        for (command, reply) in commands.iter().zip(&replies).skip(4) {
            assert!(reply.as_ref().is_err_and(|e| e.contains("Finish or cancel")), "{mode}: {command}: {reply:?}");
        }
        assert!(!save.exists(), "an unfinished edit must not be silently saved");
        assert!(render.exists(), "read-only rendering remains available while editing");
        assert_eq!(r.app.scene, before, "{mode}: control commands cannot change active geometry");
        assert_eq!((r.app.undo_depth(), r.app.dirty), (undo, dirty), "{mode}: rejected and read-only commands do not commit");
        assert_eq!(
            (r.app.sk.live, r.app.sk.source_sketch, r.app.sk.guide_edit, r.app.sk.edit.as_ref().map(|e| (e.g, e.s))),
            refs,
            "{mode}: editing targets stay bound to the same objects"
        );
        assert_eq!((r.app.sk.profile.clone(), r.app.sk.pts.clone()), (profile, points));
        if mode == "live" {
            r.app.sk.height = 63.0;
            r.app.live_update();
            r.app.sketch_off();
            assert_eq!(r.app.scene, saved, "cancelling afterwards still removes only the preview");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn control_batches_commit_one_undo_per_changed_command() {
    use serde_json::json;

    let mut r = Rig::new();
    let before = r.app.scene.clone();
    let undo = r.app.undo_depth();
    let replies = r.control(&[
        json!({"cmd": "shape.add", "kind": "Box", "name": "First command"}),
        json!({"cmd": "shape.add", "kind": "Cone", "name": "Second command"}),
    ]);
    assert!(replies.iter().all(Result::is_ok), "{replies:?}");
    assert_eq!(r.app.undo_depth(), undo + 2, "two queued changes are independently undoable");
    assert!(r.app.dirty);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 2)));
    let after = r.app.scene.clone();
    r.app.undo();
    assert_eq!(r.app.scene.shape_count(), before.shape_count() + 1);
    assert_eq!(r.app.scene.shape(0, 1).expect("first command remains").name, "First command");
    r.app.undo();
    assert_eq!(r.app.scene, before);
    r.app.redo();
    r.app.redo();
    assert_eq!(r.app.scene, after);
}

#[test]
fn control_reads_errors_and_noops_preserve_dirty_undo_and_redo() {
    use serde_json::json;

    let mut r = Rig::new();
    r.app.add_shape(&r.ctx, Kind::Box);
    r.frame(vec![]);
    let redo = r.app.scene.clone();
    r.app.undo();
    r.app.dirty = false;
    let before = r.app.scene.clone();
    let undo = r.app.undo_depth();
    let path = std::env::temp_dir().join(format!("meltcad-control-settled-save-{}.meltcad", std::process::id()));
    let replies = r.control(&[
        json!({"cmd": "commands"}),
        json!({"cmd": "scene.get"}),
        json!({"cmd": "scene.summary"}),
        json!({"cmd": "melt.set", "amount": before.melt.amount}),
        json!({"cmd": "save", "path": path}),
        json!({"cmd": "unknown"}),
        json!({"cmd": "shape.set", "group": 0, "shape": 0, "kind": "not a shape"}),
    ]);
    assert!(replies.iter().take(5).all(Result::is_ok), "{replies:?}");
    assert!(replies.iter().skip(5).all(Result::is_err), "{replies:?}");
    assert_eq!(r.app.scene, before);
    assert_eq!(r.app.undo_depth(), undo);
    assert!(!r.app.dirty);
    assert!(path.exists(), "saving a settled scene remains available");
    r.app.redo();
    assert_eq!(r.app.scene, redo, "failed, read-only and no-op commands preserve redo");
    let _ = std::fs::remove_file(path);

    // shape.add used to create a group before failing its requested group lookup.
    // An error must discard even this partial change in the command implementation.
    let mut empty = Rig::new();
    empty.app.new_scene(crate::model::Scene { groups: Vec::new(), ..crate::model::Scene::default() });
    empty.frame(vec![]);
    let before = empty.app.scene.clone();
    let reply = empty.control(&[json!({"cmd": "shape.add", "kind": "Box", "group": 999})]);
    assert!(reply[0].is_err());
    assert_eq!(empty.app.scene, before);
    assert_eq!(empty.app.undo_depth(), 0);
    assert!(!empty.app.dirty);
}

#[test]
fn control_scene_replacement_and_deletion_clear_idle_index_state() {
    use serde_json::json;

    let mut replacement = crate::model::Scene { groups: vec![crate::model::Group::new("Replacement")], ..crate::model::Scene::default() };
    replacement.groups[0].shapes.push(Shape::new(Kind::Cylinder, [10.0, 15.0, 20.0], Material::default()));
    let path = std::env::temp_dir().join(format!("meltcad-control-replace-{}.meltcad", std::process::id()));
    std::fs::write(&path, replacement.try_to_json().expect("replacement JSON")).expect("replacement file");
    for command in [
        json!({"cmd": "scene.new"}),
        json!({"cmd": "scene.set", "scene": replacement}),
        json!({"cmd": "load", "path": path}),
        json!({"cmd": "shape.delete", "group": 0, "shape": 0}),
        json!({"cmd": "group.delete", "group": 0}),
    ] {
        let mut r = Rig::new();
        r.app.add_shape(&r.ctx, Kind::Box);
        r.frame(vec![]);
        let before = r.app.scene.clone();
        let undo = r.app.undo_depth();
        r.app.dirty = false;
        r.app.sel = Some(Pick::Shape(0, 0));
        r.app.multi = vec![Pick::Shape(0, 1)];
        r.app.multi_for = r.app.sel;
        r.app.ct.node = Some((0, 0, 0));
        r.app.pt.last = Some((0, 0));
        let replies = r.control(std::slice::from_ref(&command));
        assert!(replies[0].is_ok(), "{command}: {replies:?}");
        assert_eq!(r.app.undo_depth(), undo + 1, "{command}: exactly one undo step");
        assert!(r.app.dirty);
        assert!(r.app.ct.node.is_none() && r.app.pt.last.is_none(), "{command}: old node/pipe indices must be forgotten");
        assert!(
            r.app.multi.is_empty() && (r.app.multi_for.is_none() || r.app.multi_for == r.app.sel),
            "{command}: old multiselection must not bind to replacement shapes"
        );
        if matches!(command["cmd"].as_str(), Some("scene.set" | "load")) {
            assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
            assert_eq!(r.app.scene.shape(0, 0).expect("newly loaded shape").kind, Kind::Cylinder);
        } else {
            assert!(r.app.sel.is_none(), "{command}: deletion cannot select another object at the same old index");
        }
        let after = r.app.scene.clone();
        r.app.undo();
        assert_eq!(r.app.scene, before, "{command}: undo restores the previous document geometry");
        r.app.redo();
        assert_eq!(r.app.scene, after);
        assert!(r.app.ct.node.is_none() && r.app.pt.last.is_none());
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn control_rejects_changes_during_an_actual_node_drag() {
    use serde_json::json;

    let mut r = Rig::new();
    r.full_ui = true;
    r.app.scene.groups[0].shapes.push(curve_shape());
    r.app.sel = Some(Pick::Shape(0, 1));
    r.frame(vec![]);
    r.frame(vec![]);
    let shape = r.app.scene.shape(0, 1).expect("curve");
    let nodes = layout(&r.app.cam, r.app.view_rect, shape);
    let from = nodes.nodes[1].expect("visible middle node").c;
    let to = from + vec2(22.0, -9.0);
    r.mv(from);
    r.button(from, true);
    r.mv(to);
    let drag = r.app.ct.drag.expect("the pointer owns a node drag");
    let during = r.app.scene.clone();
    let undo = r.app.undo_depth();
    let replies = r.control(&[json!({"cmd": "shape.delete", "group": 0, "shape": 1}), json!({"cmd": "scene.get"})]);
    assert!(replies[0].is_err());
    assert_eq!(replies[1], Ok(serde_json::to_value(&during).expect("scene at the control boundary")));
    // The rest of the frame continues the pointer drag and initializes its
    // inspector handles; those normal UI updates are separate from the commands.
    assert_eq!(r.app.scene.shape_count(), during.shape_count());
    assert_eq!(r.app.scene.shape(0, 0), during.shape(0, 0));
    assert_eq!(r.app.undo_depth(), undo);
    assert_eq!(r.app.ct.drag.as_ref().map(|d| (d.g, d.s, d.i)), Some((drag.g, drag.s, drag.i)));
    r.button(to, false);
    assert!(r.app.ct.drag.is_none());
    assert_eq!(r.app.undo_depth(), undo + 1, "releasing the original drag still commits exactly that gesture");
    let replies = r.control(&[json!({"cmd": "shape.delete", "group": 0, "shape": 1})]);
    assert!(replies[0].is_ok(), "the command works after the user finishes dragging");
    assert_eq!(r.app.scene.shape_count(), 1);
    assert_eq!(r.app.undo_depth(), undo + 2);
}

#[test]
fn control_preserves_an_inspector_textedit_until_its_input_is_finished() {
    use serde_json::json;

    let mut r = Rig::new();
    r.full_ui = true;
    r.frame(vec![]);
    r.frame(vec![]);
    r.time += 0.016;
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, r.size)), time: Some(r.time), ..Default::default() };
    let mut out = r.ctx.run_ui(input, |ui| r.app.frame_body(ui));
    // Locate the actual inspector's name TextEdit from its painted text, so the
    // test clicks the real field without hard-coding panel or font coordinates.
    let name = &r.app.scene.shape(0, 0).expect("selected shape").name;
    let field = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == *name && t.pos.x > r.size.x - 312.0 => Some(t.visual_bounding_rect()),
            _ => None,
        })
        .expect("the selected shape's name is visible in the inspector");
    out.textures_delta.clear();
    r.click(field.center());
    r.replace_text("Edited before control");
    assert_eq!(r.app.scene.shape(0, 0).expect("edited shape").name, "Edited before control");
    assert!(r.ctx.egui_wants_keyboard_input());
    assert!(!r.ctx.input(|i| i.pointer.any_down()));
    assert!(!r.app.sketch_active() && r.app.renaming.is_none(), "the inspector TextEdit is the only protected input");
    let before = r.app.scene.clone();
    let undo = r.app.undo_depth();
    let path = std::env::temp_dir().join(format!("meltcad-control-focused-{}.meltcad", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let replies = r.control(&[json!({"cmd": "shape.add", "kind": "Box"}), json!({"cmd": "scene.summary"}), json!({"cmd": "save", "path": path})]);
    assert!(replies[0].is_err() && replies[1].is_ok() && replies[2].is_err(), "{replies:?}");
    assert_eq!(r.app.scene, before);
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 0)));
    assert_eq!(r.app.undo_depth(), undo);
    assert!(r.ctx.egui_wants_keyboard_input());
    assert!(!path.exists(), "saving waits for the field edit too");
    r.key(egui::Key::Enter);
    assert!(!r.ctx.egui_wants_keyboard_input());
    let replies = r.control(&[json!({"cmd": "shape.add", "kind": "Box"})]);
    assert!(replies[0].is_ok());
    assert_eq!(r.app.sel, Some(Pick::Shape(0, 1)));
    assert_eq!(r.app.scene.shape(0, 0).expect("original shape").name, "Edited before control");
    assert_eq!(r.app.scene.shape(0, 1).expect("new shape").name, "Box", "the previous field's text must not carry into the new selection");
}

#[test]
fn control_animation_changes_keep_the_explicit_pose_and_one_undo_step() {
    use serde_json::json;

    for was_playing in [false, true] {
        let mut r = Rig::new();
        r.full_ui = true;
        let replies = r.control(&[
            json!({"cmd": "anim.set", "length": 9, "fps": 1}),
            json!({"cmd": "anim.key", "frame": 0}),
            json!({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [80, 0, 20]}),
            json!({"cmd": "anim.key", "frame": 8}),
            json!({"cmd": "anim.at", "frame": 0}),
        ]);
        assert!(replies.iter().all(Result::is_ok), "{replies:?}");
        let animation = r.app.scene.anim.clone();
        assert_eq!(animation.key_frames(), [0, 8]);
        r.app.an.open = true;
        r.app.an.playing = was_playing;
        let undo = r.app.undo_depth();
        let replies = r.control(&[json!({"cmd": "anim.at", "frame": 4})]);
        assert_eq!(replies[0].as_ref().expect("pose was applied")["frame"], 4);
        assert_eq!(r.app.an.frame, 4);
        assert!(!r.app.an.playing);
        assert_eq!(r.app.scene.shape(0, 0).expect("interpolated shape").pos[0], 40.0);
        assert_eq!(r.app.scene.anim, animation, "a control pose must not be captured into the old UI frame");
        assert_eq!(r.app.undo_depth(), undo + 1);
        r.frame(vec![]);
        r.frame(vec![]);
        assert_eq!(r.app.scene.anim, animation);
        assert_eq!(r.app.undo_depth(), undo + 1, "later UI frames must not add a second auto-key undo");

        // The clock, not just the displayed frame, must follow a control seek.
        r.app.an.playing = true;
        r.frame(vec![]);
        assert_eq!(r.app.an.frame, 4, "resuming at 1 fps starts from the requested frame");
        assert_eq!(r.app.scene.shape(0, 0).expect("resumed shape").pos[0], 40.0);

        let undo = r.app.undo_depth();
        let replies = r.control(&[json!({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [60, 0, 20]})]);
        assert!(replies[0].is_ok());
        assert!(!r.app.an.playing, "playback cannot immediately overwrite an external edit");
        assert_eq!(r.app.scene.shape(0, 0).expect("explicitly edited shape").pos[0], 60.0);
        assert_eq!(r.app.scene.anim, animation, "shape.set has the same explicit effect as in a CLI script");
        assert_eq!(r.app.undo_depth(), undo + 1);

        assert!(r.control(&[json!({"cmd": "anim.at", "frame": 8})])[0].is_ok());
        r.app.an.playing = true;
        let undo = r.app.undo_depth();
        assert!(r.control(&[json!({"cmd": "anim.delete", "frame": 8})])[0].is_ok());
        assert!(!r.app.an.playing);
        assert_eq!(r.app.scene.anim.key_frames(), [0], "UI auto-capture cannot recreate a deleted pose");
        assert_eq!(r.app.scene.shape(0, 0).expect("delete keeps the current geometry").pos[0], 80.0);
        assert_eq!(r.app.undo_depth(), undo + 1);
        r.frame(vec![]);
        assert_eq!(r.app.scene.anim.key_frames(), [0]);
        assert_eq!(r.app.undo_depth(), undo + 1);
        r.app.undo();
        assert_eq!(r.app.scene.anim, animation, "one undo restores the deleted pose");
    }
}

#[test]
fn control_animation_noop_seek_syncs_time_and_readonly_works_during_sketch() {
    use serde_json::json;

    let mut r = Rig::new();
    r.full_ui = true;
    let replies = r.control(&[json!({"cmd": "anim.set", "length": 6, "fps": 1}), json!({"cmd": "anim.key", "frame": 0})]);
    assert!(replies.iter().all(Result::is_ok));
    let before = r.app.scene.clone();
    let (undo, dirty) = (r.app.undo_depth(), r.app.dirty);
    r.app.an.open = true;
    r.app.an.playing = true;
    let replies = r.control(&[json!({"cmd": "anim.at", "frame": 9999})]);
    assert_eq!(replies[0].as_ref().expect("clamped seek")["frame"], 5);
    assert_eq!(r.app.scene, before, "a single stored pose is constant across the timeline");
    assert_eq!((r.app.undo_depth(), r.app.dirty), (undo, dirty), "a seek with no scene change creates no edit");
    assert_eq!(r.app.an.frame, 5, "the UI follows the command even without a scene difference");
    assert!(!r.app.an.playing);
    r.app.an.playing = true;
    r.frame(vec![]);
    assert_eq!(r.app.an.frame, 5, "the playback clock also moved to the clamped frame");
    r.app.an.playing = false;

    r.app.set_sketch_tool(true);
    let path = std::env::temp_dir().join(format!("meltcad-control-anim-read-{}.gif", std::process::id()));
    let replies = r.control(&[
        json!({"cmd": "anim.get"}),
        json!({"cmd": "anim.render", "path": path, "size": 64, "gpu": false}),
        json!({"cmd": "anim.at", "frame": 2}),
    ]);
    assert!(replies[0].is_ok() && replies[1].is_ok() && replies[2].is_err(), "{replies:?}");
    assert!(path.exists());
    assert!(r.app.sketch_active());
    assert_eq!(r.app.scene, before);
    assert_eq!((r.app.undo_depth(), r.app.dirty), (undo, dirty));
    assert_eq!(r.app.an.frame, 5, "a rejected pose command cannot move the playhead");
    let _ = std::fs::remove_file(path);
}
