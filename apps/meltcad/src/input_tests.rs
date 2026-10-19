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
