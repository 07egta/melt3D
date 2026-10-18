//! Box selection in the viewport. Two kinds, like the usual CAD pair: "Inside" takes the shapes
//! that sit completely inside the box, "Touching" takes every shape the box touches at all.
//! Drag on empty space while a kind is on; Shift adds to what is selected; Esc ends it.

use egui::{Color32, Painter, PointerButton, Pos2, Rect, Stroke, Ui};

use crate::app::App;
use crate::gizmo;
use crate::model::Pick;
use crate::sdf::{Prepared, add, mul};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marquee {
    /// Only shapes completely inside the box.
    Inside,
    /// Every shape the box touches.
    Touching,
}

/// The screen rectangle of a shape's box, and whether all eight corners are in front of the camera.
fn screen_box(app: &App, rect: Rect, g: usize, s: usize) -> Option<(Rect, bool)> {
    let sh = app.scene.shape(g, s)?;
    let local = Prepared::new(sh).inv;
    let ext = gizmo::half_extents(sh);
    let mut pts: Vec<Pos2> = Vec::with_capacity(8);
    for k in 0..8usize {
        let sign = |bit: usize| if (k >> bit) & 1 == 1 { 1.0 } else { -1.0 };
        let off = add(add(mul(local[0], sign(0) * ext[0]), mul(local[1], sign(1) * ext[1])), mul(local[2], sign(2) * ext[2]));
        if let Some(p) = app.cam.project(rect, add(sh.pos, off)) {
            if p.x.is_finite() && p.y.is_finite() {
                pts.push(p);
            }
        }
    }
    let first = *pts.first()?;
    let all = pts.len() == 8;
    let r = pts.iter().fold(Rect::from_min_max(first, first), |r, p| r.union(Rect::from_min_max(*p, *p)));
    Some((r, all))
}

/// The visible shapes that the box picks.
pub fn pick_in_box(app: &App, rect: Rect, area: Rect, mode: Marquee) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (g, gr) in app.scene.groups.iter().enumerate() {
        if !gr.visible {
            continue;
        }
        for (s, sh) in gr.shapes.iter().enumerate() {
            if !sh.visible {
                continue;
            }
            let Some((b, all)) = screen_box(app, rect, g, s) else { continue };
            let hit = match mode {
                Marquee::Inside => all && area.contains_rect(b),
                Marquee::Touching => area.intersects(b),
            };
            if hit {
                out.push((g, s));
            }
        }
    }
    out
}

impl App {
    /// Run the box selection for this frame. True when it took the press (no orbit, no handles).
    pub fn marquee_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, free: bool, shift: bool) -> bool {
        let Some(mode) = self.marquee else {
            self.marquee_from = None;
            return false;
        };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.marquee = None;
            self.marquee_from = None;
            return false;
        }
        if self.sketch_active() || self.ct.drawing || self.pt.on {
            self.marquee_from = None;
            return false;
        }
        if resp.drag_started_by(PointerButton::Primary) && free {
            self.marquee_from = ui.input(|i| i.pointer.press_origin());
        }
        let Some(from) = self.marquee_from else { return false };
        let to = ui.input(|i| i.pointer.interact_pos()).unwrap_or(from);
        if ui.input(|i| i.pointer.primary_down()) {
            self.marquee_to = to;
            return true;
        }
        // released: take what the box picks
        self.marquee_from = None;
        let area = Rect::from_two_pos(from, to);
        if area.width() > 3.0 || area.height() > 3.0 {
            let found = pick_in_box(self, rect, area, mode);
            let mut picks: Vec<Pick> = if shift { self.picks() } else { Vec::new() };
            for (g, s) in found {
                let p = Pick::Shape(g, s);
                if !picks.contains(&p) {
                    picks.push(p);
                }
            }
            self.sel = picks.first().copied();
            self.multi = picks.into_iter().skip(1).collect();
            self.multi_for = self.sel;
        }
        true
    }

    /// The box being dragged.
    pub fn marquee_paint(&self, painter: &Painter) {
        let (Some(mode), Some(from)) = (self.marquee, self.marquee_from) else { return };
        let r = Rect::from_two_pos(from, self.marquee_to);
        let col = match mode {
            Marquee::Inside => Color32::from_rgb(120, 170, 255),
            Marquee::Touching => Color32::from_rgb(120, 220, 160),
        };
        painter.rect_filled(r, 0.0, col.gamma_multiply(0.12));
        let stroke = Stroke::new(1.2, col);
        match mode {
            Marquee::Inside => {
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
            }
            Marquee::Touching => {
                // dashed: the box that only has to touch
                let (a, b, c, d) = (r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom());
                for (p, q) in [(a, b), (b, c), (c, d), (d, a)] {
                    painter.add(egui::Shape::dashed_line(&[p, q], stroke, 6.0, 4.0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use egui::{pos2, vec2};

    fn app_with_sphere() -> (App, Rect) {
        let mut app = App::new(true, None);
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Sphere);
        (app, Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)))
    }

    #[test]
    fn inside_needs_the_whole_shape_and_touching_needs_only_a_corner() {
        let (app, rect) = app_with_sphere();
        let n = app.scene.shape_count();
        assert!(n >= 1);
        // a box over the whole view holds everything either way
        assert_eq!(pick_in_box(&app, rect, rect, Marquee::Inside).len(), n);
        assert_eq!(pick_in_box(&app, rect, rect, Marquee::Touching).len(), n);
        // a speck in the middle touches the shape but does not hold it
        let speck = Rect::from_center_size(rect.center(), vec2(2.0, 2.0));
        assert!(pick_in_box(&app, rect, speck, Marquee::Inside).is_empty());
        assert!(!pick_in_box(&app, rect, speck, Marquee::Touching).is_empty());
        // a box far from everything picks nothing
        let far = Rect::from_min_size(pos2(-500.0, -500.0), vec2(10.0, 10.0));
        assert!(pick_in_box(&app, rect, far, Marquee::Touching).is_empty());
    }
}
