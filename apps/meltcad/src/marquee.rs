//! Box selection in the viewport: a left drag that starts on empty space. The direction picks the
//! kind, as in CAD: dragged to the right, "Inside" takes the shapes that sit completely inside the
//! box (solid outline); dragged to the left, "Touching" takes every shape the box touches (dashed).
//! As in Blender, Shift adds to what is selected and Ctrl takes away; Esc drops the box.

use egui::{Color32, Painter, PointerButton, Pos2, Rect, Stroke, Ui};

use crate::app::App;
use crate::gizmo;
use crate::model::Pick;
use crate::sdf::{Prepared, add, mul, sub};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marquee {
    /// Only shapes completely inside the box.
    Inside,
    /// Every shape the box touches.
    Touching,
}

/// The kind of box a drag makes: to the right only what is inside, to the left what it touches.
pub fn mode_of(from: Pos2, to: Pos2) -> Marquee {
    if to.x >= from.x { Marquee::Inside } else { Marquee::Touching }
}

/// The eight corners of a shape's box in the world.
fn corners(sh: &crate::model::Shape) -> [[f32; 3]; 8] {
    let local = Prepared::new(sh).inv;
    let ext = gizmo::half_extents(sh);
    std::array::from_fn(|k| {
        let sign = |bit: usize| if (k >> bit) & 1 == 1 { 1.0 } else { -1.0 };
        let off = add(add(mul(local[0], sign(0) * ext[0]), mul(local[1], sign(1) * ext[1])), mul(local[2], sign(2) * ext[2]));
        add(sh.pos, off)
    })
}

/// The screen rectangle around these world points, and whether all of them are in front of the camera.
fn screen_rect(app: &App, rect: Rect, world: &[[f32; 3]]) -> Option<(Rect, bool)> {
    let pts: Vec<Pos2> = world.iter().filter_map(|p| app.cam.project(rect, *p)).filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
    let first = *pts.first()?;
    let all = pts.len() == world.len();
    let r = pts.iter().fold(Rect::from_min_max(first, first), |r, p| r.union(Rect::from_min_max(*p, *p)));
    Some((r, all))
}

/// The screen rectangle of a shape's box, and whether all eight corners are in front of the camera.
fn screen_box(app: &App, rect: Rect, g: usize, s: usize) -> Option<(Rect, bool)> {
    screen_rect(app, rect, &corners(app.scene.shape(g, s)?))
}

/// The screen rectangle of a linked copy: the boxes of the shapes it shows, where its transform puts them.
fn linked_box(app: &App, rect: Rect, g: usize) -> Option<(Rect, bool)> {
    let gr = app.scene.groups.get(g)?;
    let src = app.scene.groups.get(app.scene.group_by_uid(gr.instance_of?)?)?;
    let inst = gr.xform.inst(crate::pattern::centre_of_shapes(&src.shapes));
    let world: Vec<[f32; 3]> = src.shapes.iter().filter(|s| s.visible).flat_map(corners).map(|p| inst.apply(p)).collect();
    screen_rect(app, rect, &world)
}

/// Does the shape itself (not just the box around it) show inside `area`? Rays are sent through
/// a grid of points of the part of `area` that is over the shape's box and marched along its
/// distance field, so a torus' hole or a notch is not "touched".
fn shape_shows_in(app: &App, rect: Rect, sh: &crate::model::Shape, b: Rect, area: Rect) -> bool {
    let region = area.intersect(b);
    if region.width() < 0.0 || region.height() < 0.0 {
        return false;
    }
    let prep = Prepared::new(sh);
    let ext = gizmo::half_extents(sh);
    let reach = 2.0 * (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt() + 1.0;
    let eye = app.cam.eye();
    let far = {
        let d = sub(sh.pos, eye);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() + reach
    };
    // about one ray per 3 px, at most 24 x 24; a tiny box still gets its middle
    let cols = ((region.width() / 3.0).ceil() as usize).clamp(1, 24);
    let rows = ((region.height() / 3.0).ceil() as usize).clamp(1, 24);
    let eps = (reach * 1e-3).max(1e-3);
    for j in 0..rows {
        for i in 0..cols {
            let at = Pos2::new(
                region.min.x + region.width() * (i as f32 + 0.5) / cols as f32,
                region.min.y + region.height() * (j as f32 + 0.5) / rows as f32,
            );
            let (o, d) = app.cam.ray_at(rect, at);
            let mut t = 0.0f32;
            for _ in 0..96 {
                let dist = prep.distance(add(o, mul(d, t)));
                if !dist.is_finite() {
                    break;
                }
                if dist < eps {
                    return true;
                }
                t += dist.max(eps * 0.5);
                if t > far + reach {
                    break;
                }
            }
        }
    }
    false
}

/// The visible shapes that the box picks.
pub fn pick_in_box(app: &App, rect: Rect, area: Rect, mode: Marquee) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (g, gr) in app.scene.groups.iter().enumerate() {
        if !gr.visible {
            continue;
        }
        // a linked copy has no shapes of its own: it is picked as a whole, by where it stands
        if gr.instance_of.is_some()
            && let Some((b, all)) = linked_box(app, rect, g)
        {
            let hit = match mode {
                Marquee::Inside => all && area.contains_rect(b),
                Marquee::Touching => area.intersects(b),
            };
            if hit {
                out.push((g, crate::sdf::LINKED));
            }
        }
        for (s, sh) in gr.shapes.iter().enumerate() {
            if !sh.visible {
                continue;
            }
            let Some((b, all)) = screen_box(app, rect, g, s) else { continue };
            let hit = match mode {
                Marquee::Inside => all && area.contains_rect(b),
                // the box must touch the shape's own picture; with a corner behind the camera
                // the box is only a guess, so it stays the answer
                Marquee::Touching => area.intersects(b) && (!all || shape_shows_in(app, rect, sh, b, area)),
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
        if self.sketch_active() || self.ct.drawing || self.pt.on {
            self.marquee_from = None;
            return false;
        }
        if self.marquee_from.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.marquee_from = None;
            return true;
        }
        if resp.drag_started_by(PointerButton::Primary) && free && !ui.input(|i| i.modifiers.alt) {
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
            let found = pick_in_box(self, rect, area, mode_of(from, to));
            let remove = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
            let mut picks: Vec<Pick> = if shift || remove { self.picks() } else { Vec::new() };
            for (g, s) in found {
                let p = crate::view::as_pick(g, s);
                if remove {
                    picks.retain(|q| *q != p);
                } else if !picks.contains(&p) {
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
        let Some(from) = self.marquee_from else { return };
        let mode = mode_of(from, self.marquee_to);
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

    /// #162: a box inside a torus' hole does not touch it; one over the ring does.
    #[test]
    fn touching_ignores_the_hole_of_a_torus() {
        let mut app = App::new(true, None);
        let ctx = egui::Context::default();
        app.add_shape(&ctx, Kind::Torus);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        app.cam.pitch = std::f32::consts::FRAC_PI_2 - 0.001;
        // the starting scene holds a sphere in the middle of the torus' hole: keep the torus only
        for g in &mut app.scene.groups {
            g.shapes.retain(|s| s.kind == Kind::Torus);
        }
        let pos = app.scene.groups.iter().flat_map(|g| g.shapes.iter()).next().map(|s| s.pos).unwrap();
        let c = app.cam.project(rect, pos).unwrap();
        let hole = Rect::from_center_size(c, vec2(3.0, 3.0));
        assert!(pick_in_box(&app, rect, hole, Marquee::Touching).is_empty(), "the hole is empty space");
        let big = Rect::from_center_size(c, vec2(700.0, 500.0));
        assert!(!pick_in_box(&app, rect, big, Marquee::Touching).is_empty());
    }

    #[test]
    fn a_linked_copy_is_picked_by_the_box_where_it_stands() {
        let (mut app, rect) = app_with_sphere();
        let uid = app.scene.ensure_group_uid(0).unwrap();
        let mut link = crate::model::Group::new("link");
        link.instance_of = Some(uid);
        link.xform.pos = [60.0, 0.0, 0.0];
        app.scene.groups.push(link);
        let at = app.scene.groups.len() - 1;
        let c = app.cam.project(rect, crate::pattern::centre_of_shapes(&app.scene.groups[0].shapes)).unwrap_or(rect.center());
        let spot = app.cam.project(rect, add(crate::pattern::centre_of_shapes(&app.scene.groups[0].shapes), [60.0, 0.0, 0.0])).unwrap_or(c);
        let small = Rect::from_center_size(spot, vec2(2.0, 2.0));
        let found = pick_in_box(&app, rect, small, Marquee::Touching);
        assert!(found.contains(&(at, crate::sdf::LINKED)), "{found:?}");
        // the whole view holds it with Inside as well, and it turns into a group pick
        let huge = Rect::from_min_size(pos2(-1e5, -1e5), vec2(2e5, 2e5));
        let all = pick_in_box(&app, rect, huge, Marquee::Inside);
        assert!(all.contains(&(at, crate::sdf::LINKED)));
        assert_eq!(crate::view::as_pick(at, crate::sdf::LINKED), Pick::Group(at));
    }
}
