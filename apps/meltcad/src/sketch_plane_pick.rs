//! Choosing the sketch plane the easy way: when a sketch tool starts, hover a face of a shape (or
//! the ground) and a translucent square shows the plane; the first click chooses it AND places the
//! first point, so picking a plane and drawing is one gesture.

use egui::{Color32, Painter, Pos2, Rect, Shape as Gfx, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::sdf::{self, V3};
use crate::sketch::Frame;
use crate::sketch_ui::{Pen, PlaneSel, Stage, tag};
use crate::style;

/// Where the view cube is (clicks there are not ours).
const GNOMON_PX: f32 = 58.0;
/// Half the side of the square shown on a face and on the ground (pixels).
const FACE_SQUARE_PX: f32 = 46.0;
const GROUND_SQUARE_PX: f32 = 150.0;

impl App {
    /// The viewport waits for a plane to be picked.
    pub(crate) fn picking_now(&self) -> bool {
        let st = &self.sk;
        st.on
            && st.picking
            && st.edit.is_none()
            && st.live.is_none()
            && matches!(st.stage, Stage::Draw | Stage::SweepPath)
            && st.pts.is_empty()
            && st.anchor.is_none()
    }

    /// Start picking again ("Re-pick plane"): nothing is drawn until a plane is clicked.
    pub(crate) fn sketch_repick(&mut self) {
        self.sk.clear_drawing();
        self.sk.picking = true;
        self.sk.hover_frame = None;
        self.sk.msg.clear();
    }

    /// The plane under a screen point: the face of a shape (at the hit point, along its surface
    /// normal), else the floor. The flag is true for a face; the point is where the ray landed.
    pub(crate) fn plane_under(&self, rect: Rect, p: Pos2) -> Option<(Frame, bool, V3)> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        let (o, d) = self.cam.ray_at(rect, p);
        if !o.iter().chain(d.iter()).all(|v| v.is_finite()) {
            return None;
        }
        let field = self.field();
        if let Some((t, _)) = field.ray(o, d, 5000.0)
            && t.is_finite()
        {
            let hit = sdf::add(o, sdf::mul(d, t));
            let n = crate::stroke::surface_normal(&field, hit).unwrap_or_else(|| sdf::mul(d, -1.0));
            return Some((Frame::from_hit(hit, n), true, hit));
        }
        let floor = Frame::floor();
        floor.hit(o, d).map(|q| (floor, false, floor.to_world(q)))
    }

    /// Make a picked frame the sketch plane. The camera glides to face it unless `fly` is false.
    pub(crate) fn sketch_pick_plane(&mut self, frame: Frame, face: bool, fly: bool) {
        if face {
            self.sk.custom_frame = frame;
            self.sk.custom_angles = frame.euler();
            self.sk.plane = PlaneSel::Custom;
        } else {
            self.sk.plane = PlaneSel::Floor;
        }
        self.sk.plane_square = face;
        self.sk.plane_manual = true;
        self.sk.picking = false;
        self.sk.hover_frame = None;
        self.sk.clear_drawing();
        self.sk.msg.clear();
        self.sk.frame = self.sketch_plane();
        if fly {
            let f = self.sk.frame;
            self.sketch_fly(&f);
        } else {
            self.fly = None;
        }
    }

    /// The mouse while a plane is being picked: hover shows the square, the first press picks.
    /// Afterwards the same press goes on to place the first point.
    pub(crate) fn plane_pick_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) {
        let over_gnomon = |p: Pos2| (p - pos2(rect.right() - 62.0, rect.top() + 62.0)).length() < GNOMON_PX;
        let hover = pos.filter(|p| resp.hovered() && !over_gnomon(*p));
        self.sk.hover_frame = if mods.alt { None } else { hover.and_then(|p| self.plane_under(rect, p)) };
        if mods.alt {
            return;
        }
        let drag_pen = matches!(self.sk.active_pen(), Pen::Rect | Pen::Circle);
        let press = if drag_pen && resp.drag_started_by(egui::PointerButton::Primary) {
            ui.input(|i| i.pointer.press_origin()).or(pos)
        } else if resp.clicked_by(egui::PointerButton::Primary) {
            pos
        } else {
            None
        };
        if let Some(p) = press.filter(|p| !over_gnomon(*p))
            && let Some((f, face, _)) = self.plane_under(rect, p)
        {
            // (a camera glide under a dragged rectangle would move the corner away)
            self.sketch_pick_plane(f, face, !drag_pen);
        }
    }

    /// The translucent square on the face (or ground) under the mouse.
    pub(crate) fn paint_pick(&self, p: &Painter, rect: Rect) {
        let Some((f, face, hit)) = self.sk.hover_frame else { return };
        let px = if face { FACE_SQUARE_PX } else { GROUND_SQUARE_PX };
        let half = (self.cam.mm_per_px(hit, rect.height()) * px).clamp(0.5, 2000.0);
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let pts: Vec<Pos2> = corners
            .iter()
            .filter_map(|(a, b)| {
                let w = sdf::add(hit, sdf::add(sdf::mul(f.u, a * half), sdf::mul(f.v, b * half)));
                self.cam.project(rect, w)
            })
            .collect();
        if pts.len() != 4 {
            return;
        }
        p.add(Gfx::convex_polygon(pts, style::ACCENT.gamma_multiply(0.22), Stroke::new(1.6, style::ACCENT)));
        if let Some(c) = self.cam.project(rect, hit) {
            p.circle_filled(c, 3.0, Color32::WHITE);
            tag(p, c + vec2(14.0, 12.0), if face { "Draw on this face" } else { "Draw on the floor" });
        }
    }

    /// The axes of the chosen plane (red along u, green along v), drawn at its origin.
    pub(crate) fn paint_plane_axes(&self, p: &Painter, rect: Rect, f: &Frame) {
        let len = (self.cam.mm_per_px(f.origin, rect.height()) * 70.0).clamp(0.5, 2000.0);
        let Some(o) = self.cam.project(rect, f.origin) else { return };
        for (dir, col) in [(f.u, Color32::from_rgb(225, 90, 90)), (f.v, Color32::from_rgb(90, 200, 120))] {
            if let Some(e) = self.cam.project(rect, sdf::add(f.origin, sdf::mul(dir, len))) {
                p.line_segment([o, e], Stroke::new(3.4, Color32::from_black_alpha(90)));
                p.line_segment([o, e], Stroke::new(1.8, col));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picking_ends_once_a_plane_is_chosen() {
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        assert!(app.picking_now());
        let f = Frame::from_hit([5.0, 5.0, 10.0], [1.0, 0.0, 0.0]);
        app.sketch_pick_plane(f, true, true);
        assert!(!app.picking_now());
        assert_eq!(app.sk.frame, f);
        app.sketch_repick();
        assert!(app.picking_now());
        app.sketch_set_plane(PlaneSel::Front);
        assert!(!app.picking_now(), "choosing a named plane ends picking");
    }

    #[test]
    fn hostile_hover_points_do_not_panic() {
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        for p in [pos2(f32::NAN, 3.0), pos2(1e30, -1e30), pos2(400.0, 300.0), pos2(-5.0, -5.0)] {
            let _ = app.plane_under(rect, p);
        }
    }

    #[test]
    fn the_picked_frame_follows_the_surface() {
        let up = Frame::from_hit([1.0, 2.0, 3.0], [0.0, 0.0, 1.0]);
        assert_eq!((up.u, up.v, up.origin), ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 2.0, 3.0]));
        let side = Frame::from_hit([0.0; 3], [1.0, 0.0, 0.0]);
        assert_eq!((side.u, side.v), (Frame::side().u, Frame::side().v));
        let front = Frame::from_hit([0.0; 3], [0.0, -1.0, 0.0]);
        assert_eq!((front.u, front.v), (Frame::front().u, Frame::front().v));
        for n in [[f32::NAN, 0.0, 1.0], [0.0; 3], [f32::INFINITY, 0.0, 0.0], [1e30, 1e30, 1e30]] {
            assert_eq!(Frame::from_hit([0.0; 3], n), Frame::floor());
        }
        assert_eq!(Frame::from_hit([f32::NAN, 0.0, 0.0], [0.0, 0.0, 1.0]), Frame::floor());
        // tilted faces stay orthonormal
        let t = Frame::from_hit([0.0; 3], [0.3, -0.5, 0.8]);
        assert!(sdf::dot(t.u, t.v).abs() < 1e-4);
        assert!((sdf::len(t.u) - 1.0).abs() < 1e-4 && (sdf::len(t.v) - 1.0).abs() < 1e-4);
    }
}
