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
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        crate::style::apply(&ctx);
        let mut rig = Rig { ctx, app: App::new(true, None), time: 0.0, size: vec2(1100.0, 760.0), mods: egui::Modifiers::NONE };
        rig.frame(vec![]);
        rig.frame(vec![]);
        rig
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        self.time += 0.016;
        events.insert(0, Event::ModifiersChanged(self.mods));
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), events, time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| {
            // Match frame_body ordering: keys, canvas, then the interactive overlay.
            app.keys(ui.ctx());
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| app.viewport(ui));
            app.sketch_card(ui.ctx());
            app.settle(ui.ctx());
        });
        out.textures_delta.clear();
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
    }

    fn sketch(&mut self, pen: Pen) {
        self.app.set_sketch_tool(true);
        self.app.sk.pen = pen;
        self.app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
        self.app.cam.target = [0.0, 0.0, 0.0];
        self.app.cam.dist = 140.0;
        self.app.fly = None;
        // Populate the card's hit-test layer before sending input events.
        self.frame(vec![]);
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

#[test]
fn sketch_polygon_clicks_and_enter_complete_without_changing_selection() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let selected = r.app.sel;
    for q in [[-24.0, -17.0], [26.0, -17.0], [24.0, 18.0]] {
        let p = r.on_plane(q);
        r.click(p);
    }
    assert_eq!(r.app.sk.pts.len(), 3, "three pointer clicks make three corners");
    assert_eq!(r.app.sel, selected, "drawing must not pick the solid behind the sketch");
    r.key(egui::Key::Enter);
    assert_eq!(r.app.sk.stage, Stage::Ready);
    assert_eq!(r.app.sk.profile.len(), 3);
    assert!(r.app.sk.pts.is_empty());
}

#[test]
fn sketch_double_click_finishes_line_without_duplicate_endpoint() {
    let mut r = Rig::new();
    r.sketch(Pen::Line);
    r.app.sk.smooth = false;
    let a = r.on_plane([-30.0, -20.0]);
    let b = r.on_plane([25.0, -12.0]);
    r.click(a);
    assert!(r.app.sk.on, "line tool remains active after first click");
    assert_eq!(r.app.sk.pts.len(), 1, "first line point; {:?}", r.app.sk.msg);
    r.click(b);
    assert_eq!(r.app.sk.pts.len(), 2, "second line point; {:?}", r.app.sk.msg);
    r.click(b);
    assert_eq!(r.app.sk.stage, Stage::Draw, "an open line stays in the sketch tool");
    assert_eq!(r.app.sk.path.len(), 2, "the second click at the endpoint must not add another node");
    assert!(r.app.sk.pts.is_empty());
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
        assert_eq!(r.app.sk.stage, Stage::Ready, "{pen:?} drag must finish");
        assert_eq!(r.app.sk.profile.len(), sides);
        assert!(!r.app.sk.dragging && r.app.sk.anchor.is_none());
        assert_eq!((r.app.cam.yaw, r.app.cam.pitch), (yaw, pitch), "sketch drag must not orbit");
    }
}

#[test]
fn sketch_rectangle_enter_finishes_a_two_click_preview() {
    let mut r = Rig::new();
    r.sketch(Pen::Rect);
    let a = r.on_plane([-25.0, -15.0]);
    let b = r.on_plane([21.0, 18.0]);
    r.click(a);
    assert!(r.app.sk.anchor.is_some());
    r.mv(b);
    r.key(egui::Key::Enter);
    assert_eq!(r.app.sk.stage, Stage::Ready);
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
    assert!(!r.app.sk.dragging);
    assert!(r.app.sk.anchor.is_none(), "no phantom first corner after PointerGone");
    r.button(pos2(-50.0, -50.0), false);
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
    let card_rect = r.ctx.memory(|m| m.area_rect(egui::Id::new("sketch_card"))).expect("visible sketch card");
    let card = card_rect.min + vec2(4.0, 4.0);
    r.click(card);
    assert!(r.app.sk.pts.is_empty(), "card header must not place a sketch point");
    assert!(r.app.sk.on, "the test clicks card padding, not the end-tool button");
    assert_eq!(r.app.sel, selected);
    let canvas = r.on_plane([20.0, -15.0]);
    assert!(!card_rect.contains(canvas), "the drawing point is outside the card");
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
fn escape_without_a_gizmo_drag_still_clears_selection() {
    let mut r = Rig::new();
    let before = r.app.scene.clone();
    assert!(r.app.sel.is_some());
    r.key(egui::Key::Escape);
    assert!(r.app.sel.is_none(), "idle Escape reaches the ordinary shortcut handler");
    assert_eq!(r.app.scene, before);
}

#[test]
fn escape_still_reaches_the_curve_tool() {
    let mut r = Rig::new();
    r.app.ct.drawing = true;
    let selected = r.app.sel;
    r.key(egui::Key::Escape);
    assert!(!r.app.ct.drawing, "Escape ends an empty curve drawing gesture");
    assert_eq!(r.app.sel, selected, "the curve tool owns its cancellation key");
}

#[test]
fn sketch_gnomon_click_changes_view_without_drawing_or_picking() {
    let mut r = Rig::new();
    r.sketch(Pen::Polygon);
    let selected = r.app.sel;
    // Looking straight down, the +X button is 34 logical pixels right of the gnomon centre.
    let x_axis = pos2(r.app.view_rect.right() - 28.0, r.app.view_rect.top() + 62.0);
    r.click(x_axis);
    let goal = r.app.fly.expect("the gnomon must still orient the view while sketching");
    assert!((goal.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!(goal.pitch.abs() < 1e-5);
    assert!(r.app.sk.pts.is_empty(), "a view button must not add a sketch point");
    assert_eq!(r.app.sel, selected);
}

#[test]
fn escape_during_handle_drag_restores_the_shape_and_clears_drag() {
    let mut r = Rig::new();
    r.app.tool = crate::gizmo::Tool::Scale;
    let start = r.app.scene.clone();
    let selected = r.app.sel;
    let from = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("centre");
    r.mv(from);
    r.button(from, true);
    r.mv(from + vec2(90.0, 60.0));
    assert!(r.app.drag.is_some(), "pointer events started a gizmo drag");
    assert_ne!(r.app.scene, start, "the drag preview changes geometry before cancellation");
    r.key(egui::Key::Escape);
    assert!(r.app.drag.is_none(), "Escape cancels the active drag");
    assert_eq!(r.app.scene, start, "Escape restores the pre-drag geometry");
    assert_eq!(r.app.sel, selected, "cancellation keeps the selection");
    r.button(from + vec2(90.0, 60.0), false);
    r.frame(vec![]);
    assert_eq!(r.app.scene, start, "release after cancellation must not mutate the scene");
    assert!(!r.app.dirty, "a cancelled gesture is not a committed edit");
}

#[test]
fn a_gizmo_drag_is_exactly_one_undo_step() {
    for tool in [crate::gizmo::Tool::Move, crate::gizmo::Tool::Scale] {
        let mut r = Rig::new();
        r.app.tool = tool;
        let start = r.app.scene.clone();
        let centre = r.app.cam.project(r.app.view_rect, [0.0, 0.0, 20.0]).expect("centre");
        r.drag(centre, centre + vec2(90.0, -60.0));
        assert_ne!(r.app.scene, start, "{tool:?} changes the shape");
        let end = r.app.scene.clone();
        r.mods = egui::Modifiers::COMMAND;
        r.key(egui::Key::Z);
        assert_eq!(r.app.scene, start, "one Undo restores the entire {tool:?} gesture");
        r.mods = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        r.key(egui::Key::Z);
        assert_eq!(r.app.scene, end, "Redo restores the completed {tool:?} gesture");
    }
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
fn delete_without_curve_node_selection_still_deletes_selected_shape() {
    let mut r = Rig::new();
    let n = r.app.scene.shape_count();
    r.key(egui::Key::Delete);
    assert_eq!(r.app.scene.shape_count(), n - 1, "Delete regression");
    r.mods = egui::Modifiers::COMMAND;
    r.key(egui::Key::Z);
    assert_eq!(r.app.scene.shape_count(), n, "deletion remains undoable");
}

#[test]
fn a_curve_click_refreshes_the_hover_without_an_extra_mouse_move() {
    let mut r = Rig::new();
    r.app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
    r.app.cam.target = [0.0, 0.0, 0.0];
    r.app.cam.dist = 140.0;
    r.app.fly = None;
    r.app.ct.drawing = true;
    let point = |r: &Rig, world| r.app.cam.project(r.app.view_rect, world).expect("curve point on screen");
    let a = point(&r, [35.0, -25.0, 8.0]);
    let b = point(&r, [65.0, -25.0, 8.0]);
    r.click(a);
    r.click(b);
    let (origin, direction) = r.app.cam.ray_at(r.app.view_rect, b);
    let expected = crate::sdf::Field::new(&r.app.scene)
        .ray(origin, direction, 5000.0)
        .and_then(|(_, sample)| sample.nearest)
        .map(|(g, s)| crate::view::as_pick(g, s));
    assert_eq!(expected, r.app.sel, "the new segment is under its endpoint");
    assert!(expected.is_some());
    assert_eq!(r.app.hover, expected, "the click uses the new geometry, even with the pointer stationary afterwards");
}

#[test]
fn zoom_past_a_foreground_shape_refreshes_hover_under_a_stationary_pointer() {
    let mut r = Rig::new();
    let mut front = Shape::new(Kind::Sphere, [0.0, 0.0, 100.0], Material::default());
    front.size[0] = 10.0;
    r.app.scene.groups[0].shapes.push(front);
    r.app.sel = None;
    r.app.cam.look(0.0, std::f32::consts::FRAC_PI_2);
    r.app.cam.target = [0.0, 0.0, 0.0];
    r.app.cam.dist = 140.0;
    r.app.fly = None;
    let pointer = r.app.view_rect.center();
    r.mv(pointer);
    r.frame(vec![]);
    assert_eq!(r.app.hover, Some(Pick::Shape(0, 1)), "the foreground sphere is initially under the pointer");
    r.frame(vec![Event::Zoom(2.0)]);
    // The camera crosses the foreground sphere. No new pointer motion is sent;
    // let the normal hover throttle's requested follow-up frame become due.
    for _ in 0..4 {
        r.frame(vec![]);
    }
    assert!(r.app.cam.eye()[2] < 90.0, "the zoom moved the camera past the foreground sphere");
    let (origin, direction) = r.app.cam.ray_at(r.app.view_rect, pointer);
    let expected = crate::sdf::Field::new(&r.app.scene)
        .ray(origin, direction, 5000.0)
        .and_then(|(_, sample)| sample.nearest)
        .map(|(g, s)| crate::view::as_pick(g, s));
    assert_eq!(expected, Some(Pick::Shape(0, 0)), "the original sphere is now visible behind it");
    assert_eq!(r.app.hover, expected, "zoom refreshes hover without a mouse move");
}
