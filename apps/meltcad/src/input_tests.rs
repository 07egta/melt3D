//! Whole-viewport input tests: real pointer events go through `App::viewport`, the way a mouse
//! does in the window (the unit tests of each tool call its methods directly and cannot see
//! problems in how the viewport hands the mouse to them).

use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

use crate::app::App;
use crate::curve_tool::layout;
use crate::model::{Kind, Material, Pick, Shape};

struct Rig {
    ctx: egui::Context,
    app: App,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        crate::style::apply(&ctx);
        let mut rig = Rig { ctx, app: App::new(true, None), time: 0.0 };
        rig.frame(vec![]);
        rig.frame(vec![]);
        rig
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.016;
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 760.0))), events, time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| app.viewport(ui));
        });
        out.textures_delta.clear();
    }

    fn mv(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn button(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
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
fn with_the_pipe_tool_still_on_a_press_on_a_node_moves_the_node_instead_of_tracing() {
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
    assert!(r.app.pt.on, "the tool is still on, as after a stroke");
    r.drag(node.c, node.c + vec2(0.0, -50.0));
    let after = r.app.scene.shape(g, s).and_then(|s| s.path.get(mid).copied());
    assert_eq!(r.app.scene.shape_count(), count, "no second pipe was traced");
    assert_ne!(before, after, "the node must move even though the pipe tool is on (before {before:?}, after {after:?})");
}
