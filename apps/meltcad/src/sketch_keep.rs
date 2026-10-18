//! A sketch stays a sketch: when the outline closes it is kept in the scene as a thin closed line
//! (a `Curve`), selected. Extrude, Loft or Revolve on a selected closed line then make the solid,
//! with the same height handle as before. Nothing is forced on the person right after drawing.

use crate::app::App;
use crate::curve;
use crate::model::{Kind, MAX_PATH, MAX_SHAPES, Material, Op, Pick, Shape};
use crate::sdf::{self, V3};
use crate::sketch::{self, Frame};
use crate::sketch_ui::{Make, Stage};

/// Radius of the line a kept sketch is drawn with (mm).
const LINE_RADIUS: f32 = 0.6;
/// A profile with at least this many points was drawn round (circle, smooth spline).
const ROUND_AT: usize = 16;

/// `pts` thinned evenly to at most `max` points (all of them when there are few enough).
pub fn thin(pts: &[[f32; 2]], max: usize) -> Vec<[f32; 2]> {
    if pts.len() <= max || max < 3 {
        return pts.to_vec();
    }
    (0..max).filter_map(|i| pts.get(i * pts.len() / max).copied()).collect()
}

/// The unit normal of the (nearly flat) loop of points, by Newell's method. None when the loop has
/// no area or the numbers are not finite. It points up when the loop is not standing.
pub fn loop_normal(pts: &[V3]) -> Option<V3> {
    if pts.len() < 3 || !pts.iter().all(|p| p.iter().all(|x| x.is_finite())) {
        return None;
    }
    let mut n = [0.0f32; 3];
    for (i, a) in pts.iter().enumerate() {
        let b = pts.get((i + 1) % pts.len())?;
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = sdf::dot(n, n).sqrt();
    if !l.is_finite() || l < 1e-6 {
        return None;
    }
    let n = sdf::mul(n, 1.0 / l);
    // up when level; for a standing loop keep a fixed side so that the result does not flip
    let up = if n[2].abs() >= 0.02 { n[2] >= 0.0 } else { n[1] < -1e-3 || (n[1].abs() <= 1e-3 && n[0] >= 0.0) };
    Some(if up { n } else { sdf::mul(n, -1.0) })
}

impl App {
    /// The outline just closed: keep it as a line, selected, and go on sketching.
    pub(crate) fn sketch_keep_outline(&mut self) -> bool {
        if self.scene.shape_count() >= MAX_SHAPES {
            self.sk.msg = format!("Up to {MAX_SHAPES} shapes");
            return false;
        }
        let pts = thin(&self.sk.profile, MAX_PATH);
        if pts.len() < 3 {
            return false;
        }
        let f = self.sk.frame;
        let world: Vec<V3> = pts.iter().map(|p| f.to_world(*p)).collect();
        if !world.iter().all(|p| p.iter().all(|x| x.is_finite())) {
            return false;
        }
        let Some(g) = self.target_group() else { return false };
        let mut sh = Shape::new(Kind::Curve, f.origin, Material { color: [0.98, 0.78, 0.3], ..Material::default() });
        sh.path = world.iter().map(|w| [w[0] - f.origin[0], w[1] - f.origin[1], w[2] - f.origin[2], LINE_RADIUS]).collect();
        sh.params = Kind::Curve.default_params();
        sh.params[0] = 1.0; // closed
        sh.params[1] = if pts.len() >= ROUND_AT { 1.0 } else { 0.0 };
        sh.params[2] = 0.0;
        sh.params[3] = 1.0;
        sh.name = "Sketch".to_string();
        sh.op = Op::Add;
        sh.blend = 0.0;
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        group.shapes.push(sh);
        let idx = group.shapes.len() - 1;
        self.sel = Some(Pick::Shape(g, idx));
        self.sk.clear_drawing();
        self.sk.profile.clear();
        self.sk.stage = Stage::Draw;
        self.sk.msg.clear();
        true
    }

    /// The selected shape when it is a closed line of at least three nodes.
    pub(crate) fn selected_closed_curve(&self) -> Option<(usize, usize)> {
        let Some(Pick::Shape(g, s)) = self.sel else { return None };
        let sh = self.scene.shape(g, s)?;
        (sh.kind == Kind::Curve && sh.path.len() >= 3 && curve::settings(&sh.params).0).then_some((g, s))
    }

    /// The plane and the flat outline of a closed line (its nodes, as they are).
    pub(crate) fn curve_profile(&self, g: usize, s: usize) -> Option<(Frame, Vec<[f32; 2]>)> {
        let sh = self.scene.shape(g, s)?;
        let world: Vec<V3> = sh.path.iter().map(|n| crate::curve_tool::to_world(sh, [n[0], n[1], n[2]])).collect();
        let normal = loop_normal(&world)?;
        let mut c = [0.0f32; 3];
        for w in &world {
            c = sdf::add(c, *w);
        }
        let c = sdf::mul(c, 1.0 / world.len() as f32);
        let f = Frame::from_hit(c, normal);
        let flat: Vec<[f32; 2]> = world
            .iter()
            .map(|w| {
                let d = sdf::sub(*w, f.origin);
                [sdf::dot(d, f.u), sdf::dot(d, f.v)]
            })
            .collect();
        Some((f, flat))
    }

    /// A strip button (Extrude, Loft, Revolve) with a closed line selected: make the solid from it
    /// now. True when the button was used up this way (the message says why when it was not made).
    pub(crate) fn make_from_selected_line(&mut self, ctx: &egui::Context, m: Make) -> bool {
        if m == Make::Sweep || self.sk.live.is_some() {
            return false;
        }
        let Some((g, s)) = self.selected_closed_curve() else { return false };
        let Some((frame, profile)) = self.curve_profile(g, s) else { return false };
        if !self.sk.on {
            self.set_sketch_tool(true);
        }
        self.sk.picking = false;
        self.sk.hover_frame = None;
        self.sk.clear_drawing();
        self.sk.profile = profile;
        self.sk.profile2.clear();
        self.sk.holes.clear();
        self.sk.pframe = frame;
        self.sk.frame = frame;
        self.sk.make = m;
        if self.live_begin() {
            return true;
        }
        let msg = self.sk.msg.clone();
        self.sk.profile.clear();
        self.say(ctx, if msg.is_empty() { "Could not make a solid from this line".to_string() } else { msg });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinning_keeps_the_ends_and_the_order() {
        let pts: Vec<[f32; 2]> = (0..100).map(|i| [i as f32, 0.0]).collect();
        let t = thin(&pts, 48);
        assert_eq!(t.len(), 48);
        assert_eq!(t[0], [0.0, 0.0]);
        assert!(t.windows(2).all(|w| w[0][0] < w[1][0]));
        assert_eq!(thin(&pts[..10], 48).len(), 10);
    }

    #[test]
    fn a_loop_points_up_whichever_way_it_was_drawn() {
        let ccw = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]];
        let mut cw = ccw;
        cw.reverse();
        assert!(loop_normal(&ccw).is_some_and(|n| n[2] > 0.99));
        assert!(loop_normal(&cw).is_some_and(|n| n[2] > 0.99));
        assert!(loop_normal(&[[0.0; 3], [1.0, 0.0, 0.0]]).is_none());
        assert!(loop_normal(&[[0.0; 3], [1.0, 0.0, 0.0], [f32::NAN, 0.0, 0.0]]).is_none());
        assert!(loop_normal(&[[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]).is_none());
    }
}
