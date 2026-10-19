//! A sketch is confirmed before a separate solid operation begins. Saved guide curves
//! retain their drawing axes and every outline vertex, including across save/reload.

use crate::app::App;
use crate::curve;
use crate::model::{Kind, MAX_SHAPES, Pick, Shape};
use crate::sdf::{self, V3};
use crate::sketch::{self, Frame};
use crate::sketch_ui::{Make, Stage};

impl App {
    /// Commit the completed outline as a guide. Numeric edits still live in the
    /// draft until this point, so there is only one geometry and one undo step.
    pub(crate) fn sketch_keep_outline(&mut self) -> bool {
        let profile = match sketch::validate_profile(&self.sk.profile) {
            Ok(profile) => profile,
            Err(e) => {
                self.sk.msg = e.message();
                return false;
            }
        };
        if let Err(e) = sketch::validate_holes(&profile, &self.sk.holes) {
            self.sk.msg = e.message();
            return false;
        }
        self.keep_guide(&profile, true)
    }

    fn keep_guide(&mut self, points: &[[f32; 2]], closed: bool) -> bool {
        let editing = self.sk.guide_edit.filter(|(g, s)| self.scene.shape(*g, *s).is_some_and(|sh| sh.guide && sh.kind == Kind::Curve));
        if self.sk.guide_edit.is_some() && editing.is_none() {
            self.sk.msg = "This sketch no longer exists. Cancel and select another sketch".into();
            return false;
        }
        if editing.is_none() && self.scene.shape_count() >= MAX_SHAPES {
            self.sk.msg = format!("Up to {MAX_SHAPES} shapes");
            return false;
        }
        let mut sh = match crate::sketch_saved::create(self.sk.pframe, points, &self.sk.holes, closed) {
            Ok(sh) => sh,
            Err(e) => {
                self.sk.msg = e;
                return false;
            }
        };
        let Some(g) = editing.map(|(g, _)| g).or_else(|| self.target_group()) else { return false };
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        let idx = if let Some((_, s)) = editing {
            let Some(original) = group.shapes.get_mut(s) else { return false };
            sh.name.clone_from(&original.name);
            sh.uid = original.uid;
            sh.material = original.material;
            sh.visible = original.visible;
            sh.source.clone_from(&original.source);
            *original = sh;
            s
        } else {
            group.shapes.push(sh);
            group.shapes.len() - 1
        };
        self.sel = Some(Pick::Shape(g, idx));
        self.multi.clear();
        self.sk.clear_drawing();
        self.sk.profile.clear();
        self.sk.stage = Stage::Draw;
        self.sk.msg.clear();
        true
    }

    /// Confirm the current command. Both the visible button and the viewport's
    /// right-click use this, so invalid numeric text can never confirm stale geometry.
    pub(crate) fn sketch_confirm(&mut self, ctx: &egui::Context) -> bool {
        if let Some(d) = &mut self.sk.dimension {
            let profile = if d.top_profile { &self.sk.profile2 } else { &self.sk.profile };
            if !d.accept(&self.sk.pts, profile) {
                self.sk.msg = d.error.clone().unwrap_or_else(|| "Check the dimensions before finishing".into());
                return false;
            }
        }
        if self.sk.live.is_some() {
            self.live_accept(ctx);
            return self.sk.live.is_none();
        }
        let stage = self.sk.stage;
        if stage != Stage::Ready {
            if let Some(done) = self.sk.enter() {
                self.sketch_done(done);
            } else {
                return false;
            }
            if stage != Stage::Draw {
                return true;
            }
        }
        if self.sk.keep_sketch && self.sk.stage == Stage::Ready {
            let saved = if !self.sk.profile.is_empty() {
                self.sketch_keep_outline()
            } else {
                let points: Vec<_> = self.sk.path.iter().map(|p| self.sk.pframe.from_world(*p)).collect();
                self.keep_guide(&points, false)
            };
            if saved {
                self.set_sketch_tool(false);
                self.say(ctx, "Sketch saved. Select it, then choose Extrude, Revolve, Loft or Sweep".to_string());
            }
            return saved;
        }
        self.sk.stage == Stage::Ready && self.sketch_make(ctx)
    }

    /// The selected shape when it is a closed line of at least three nodes.
    pub(crate) fn selected_closed_curve(&self) -> Option<(usize, usize)> {
        let Some(Pick::Shape(g, s)) = self.sel else { return None };
        let sh = self.scene.shape(g, s)?;
        (self.scene.groups.get(g)?.visible && sh.visible && sh.kind == Kind::Curve && sh.path.len() >= 3 && curve::settings(&sh.params).0)
            .then_some((g, s))
    }

    /// An unambiguous fallback when nothing is selected. Multiple sketches require
    /// an explicit choice instead of silently using whichever happens to be last.
    pub(crate) fn last_closed_guide(&self) -> Option<(usize, usize)> {
        let mut found = None;
        for (gi, g) in self.scene.groups.iter().enumerate() {
            if !g.visible {
                continue;
            }
            for (si, sh) in g.shapes.iter().enumerate() {
                if sh.guide && sh.visible && sh.kind == Kind::Curve && sh.path.len() >= 3 && curve::settings(&sh.params).0 {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((gi, si));
                }
            }
        }
        found
    }

    /// The plane and the flat outline of a closed line (its nodes, as they are).
    pub(crate) fn curve_profile(&self, g: usize, s: usize) -> Option<(Frame, Vec<[f32; 2]>)> {
        crate::sketch_saved::profile(self.scene.shape(g, s)?)
    }

    /// A strip button (Extrude, Loft, Revolve) with a closed line selected: make the solid from it
    /// now. True when the button was used up this way (the message says why when it was not made).
    pub(crate) fn make_from_selected_line(&mut self, ctx: &egui::Context, m: Make) -> bool {
        if self.sk.live.is_some() {
            return false;
        }
        if self.sk.on && self.sk.keep_sketch && (!self.sk.profile.is_empty() || !self.sk.pts.is_empty() || self.sk.anchor.is_some()) {
            self.sk.msg = "Finish Sketch first (right-click), then choose a solid operation".to_string();
            return true;
        }
        if m == Make::Sweep
            && let Some(Pick::Shape(g, s)) = self.sel
            && self.scene.groups.get(g).is_some_and(|group| group.visible)
            && self.scene.shape(g, s).is_some_and(|sh| sh.visible && sh.kind == Kind::Curve && !curve::settings(&sh.params).0)
        {
            self.set_sketch_tool(true);
            self.sk.separate = true;
            self.sk.source_sketch = Some((g, s));
            self.sketch_choose_make(m);
            return true;
        }
        let found = self.selected_closed_curve().or_else(|| self.sel.is_none().then(|| self.last_closed_guide()).flatten());
        let Some((g, s)) = found else { return false };
        self.sel = Some(Pick::Shape(g, s));
        let Some((frame, profile)) = self.curve_profile(g, s) else {
            self.say(ctx, "Choose a closed sketch on one plane".to_string());
            return true;
        };
        let holes = self.curve_holes(g, s, &frame);
        if !self.sk.on {
            self.set_sketch_tool(true);
        }
        self.sk.picking = false;
        self.sk.hover_frame = None;
        self.sk.clear_drawing();
        self.sk.profile = profile;
        self.sk.profile2.clear();
        self.sk.path.clear();
        self.sk.holes = holes;
        self.sk.pframe = frame;
        self.sk.frame = frame;
        self.sk.make = m;
        self.sk.keep_sketch = false;
        self.sk.separate = true;
        self.sk.source_sketch = Some((g, s));
        self.sk.secondary_sketch = None;
        self.sk.sweep_round = false;
        self.sk.loft_top = crate::sketch_ui::LoftTop::Scaled;
        self.sk.stage = Stage::Ready;
        if (m == Make::Sweep && self.sk.path.len() < 2) || !self.sk.holes.is_empty() {
            return true;
        }
        if self.live_begin() {
            return true;
        }
        // Keep the selected profile and the reason visible so the user can correct
        // the operation or cancel without losing the saved sketch.
        true
    }

    pub(crate) fn curve_holes(&self, g: usize, s: usize, frame: &Frame) -> Vec<Vec<[f32; 2]>> {
        let Some(sh) = self.scene.shape(g, s) else { return Vec::new() };
        crate::sketch_saved::holes(sh, frame)
    }

    /// Use another finished sketch without entering a new drawing gesture. Loft
    /// keeps the actual distance and offset between parallel sketch planes.
    pub(crate) fn use_secondary_sketch(&mut self, g: usize, s: usize) -> bool {
        if !self.scene.groups.get(g).is_some_and(|group| group.visible) {
            self.sk.msg = "Choose a visible, finished sketch".into();
            return false;
        }
        if self.sk.source_sketch == Some((g, s)) {
            self.sk.msg = "Choose a different sketch".into();
            return false;
        }
        let Some(sh) = self.scene.shape(g, s).filter(|sh| sh.guide && sh.visible && sh.kind == Kind::Curve) else {
            self.sk.msg = "Choose a visible, finished sketch".into();
            return false;
        };
        match self.sk.make {
            Make::Loft => {
                let (top, height, flip) = match crate::sketch_saved::loft_top(&self.sk.pframe, &self.sk.profile, sh) {
                    Ok(top) => top,
                    Err(e) => {
                        self.sk.msg = e;
                        return false;
                    }
                };
                self.sk.profile2 = top;
                self.sk.height = height;
                self.sk.flip = flip;
                self.sk.loft_top = crate::sketch_ui::LoftTop::Drawn;
            }
            Make::Sweep => {
                let path = match crate::sketch_saved::open_path(sh) {
                    Ok(path) => path,
                    Err(e) => {
                        self.sk.msg = e;
                        return false;
                    }
                };
                if let Err(e) = sketch::sweep(&self.sk.profile, &path, self.sk.sweep_scale) {
                    self.sk.msg = e.message();
                    return false;
                }
                self.sk.path = path;
                self.sk.sweep_round = false;
            }
            _ => return false,
        }
        self.sk.secondary_sketch = Some((g, s));
        self.sk.msg.clear();
        true
    }

    /// Inputs for Loft and profile Sweep. Drawing and solid creation stay separate.
    /// Returns whether the live preview needs to be rebuilt.
    pub(crate) fn saved_sketch_inputs(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::sketch_ui::LoftTop;
        if !self.sk.separate || !matches!(self.sk.make, Make::Loft | Make::Sweep) || self.sk.sweep_round {
            return false;
        }
        let mut changed = false;
        if self.sk.make == Make::Loft {
            ui.horizontal_wrapped(|ui| {
                ui.label("Top outline");
                if crate::style::pill(ui, "Scaled copy", self.sk.loft_top == LoftTop::Scaled).clicked() {
                    self.sk.loft_top = LoftTop::Scaled;
                    self.sk.secondary_sketch = None;
                    self.sk.profile2.clear();
                    changed = true;
                }
                if crate::style::pill(ui, "Saved sketch", self.sk.loft_top == LoftTop::Drawn).clicked() {
                    self.sk.loft_top = LoftTop::Drawn;
                    changed = true;
                }
            });
            if self.sk.loft_top == LoftTop::Scaled {
                return changed;
            }
        }
        let closed = self.sk.make == Make::Loft;
        let mut candidates = Vec::new();
        for (g, group) in self.scene.groups.iter().enumerate().filter(|(_, group)| group.visible) {
            for (s, sh) in group.shapes.iter().enumerate() {
                if sh.guide
                    && sh.visible
                    && sh.kind == Kind::Curve
                    && Some((g, s)) != self.sk.source_sketch
                    && curve::settings(&sh.params).0 == closed
                {
                    candidates.push(((g, s), format!("{} / {} · {}", group.name, sh.name, s + 1)));
                }
            }
        }
        let selected = self.sk.secondary_sketch;
        let label = candidates.iter().find(|(id, _)| Some(*id) == selected).map(|(_, label)| label.as_str()).unwrap_or(if closed {
            "Choose a top sketch"
        } else {
            "Choose a saved path"
        });
        let mut picked = None;
        egui::ComboBox::from_id_salt("solid_saved_input").selected_text(label).width(ui.available_width().min(320.0)).show_ui(ui, |ui| {
            for (id, label) in &candidates {
                if ui.selectable_label(Some(*id) == selected, label).clicked() {
                    picked = Some(*id);
                }
            }
        });
        if let Some((g, s)) = picked {
            changed |= self.use_secondary_sketch(g, s);
        }
        if candidates.is_empty() {
            ui.add(
                egui::Label::new(if closed {
                    "Save a second sketch on a parallel plane, then choose Loft."
                } else {
                    "Save an open sketch with Line first, then choose Sweep."
                })
                .wrap(),
            );
        }
        changed
    }

    /// Edit a saved closed sketch without changing the scene until Finish Sketch.
    pub(crate) fn edit_saved_sketch(&mut self, g: usize, s: usize) -> bool {
        if !self.scene.shape(g, s).is_some_and(|sh| sh.guide) {
            return false;
        }
        let Some((frame, profile)) = self.curve_profile(g, s) else { return false };
        let holes = self.curve_holes(g, s, &frame);
        self.set_sketch_tool(true);
        self.sk.picking = false;
        self.sk.guide_edit = Some((g, s));
        self.sk.frame = frame;
        self.sk.pframe = frame;
        self.sk.profile = profile;
        self.sk.holes = holes;
        self.sk.stage = Stage::Ready;
        self.sel = Some(Pick::Shape(g, s));
        self.multi.clear();
        if let Some((lo, hi)) = sketch::bbox(&self.sk.profile) {
            if sketch::rectangle_size(&self.sk.profile).is_some() {
                self.sk.pen = crate::sketch_ui::Pen::Rect;
                self.sk.dimension = crate::sketch_dimensions::Dimensions::for_shape(self.sk.pen, lo, hi, &self.sk.profile, false);
            } else if let Some(diameter) = sketch::circle_diameter(&self.sk.profile) {
                self.sk.pen = crate::sketch_ui::Pen::Circle;
                let c = sketch::centre(&self.sk.profile);
                self.sk.dimension =
                    crate::sketch_dimensions::Dimensions::for_shape(self.sk.pen, c, [c[0] + diameter * 0.5, c[1]], &self.sk.profile, false);
            }
        }
        true
    }
}

/// The points of a sketch line in the world, closed again when the line is a loop.
pub fn guide_points(sh: &Shape) -> Vec<V3> {
    let (closed, ..) = curve::settings(&sh.params);
    let mut pts: Vec<V3> = curve::build(&sh.path, &sh.params).iter().map(|n| crate::curve_tool::to_world(sh, [n[0], n[1], n[2]])).collect();
    if closed
        && let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied())
        && sdf::dot(sdf::sub(first, last), sdf::sub(first, last)) > 1e-6
    {
        pts.push(first);
    }
    pts.retain(|p| p.iter().all(|x| x.is_finite()));
    pts
}

fn guide_loops(sh: &Shape) -> Vec<Vec<V3>> {
    let mut loops = vec![guide_points(sh)];
    if let Some(plane) = &sh.guide_plane {
        for hole in &plane.holes {
            let mut points: Vec<_> =
                hole.iter().map(|q| crate::curve_tool::to_world(sh, sdf::add(sdf::mul(plane.u, q[0]), sdf::mul(plane.v, q[1])))).collect();
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            loops.push(points);
        }
    }
    loops
}

/// Distance in pixels from `p` to the segment `a`-`b`.
fn seg_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 > 1e-6 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p - (a + ab * t)).length()
}

/// Pixels within which a click picks a sketch line.
const PICK_PX: f32 = 7.0;

impl App {
    /// Draw the sketch lines over the picture (they are not part of the rendered solid).
    pub(crate) fn guide_paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        let picks = self.picks();
        for (gi, g) in self.scene.groups.iter().enumerate() {
            if !g.visible {
                continue;
            }
            for (si, sh) in g.shapes.iter().enumerate() {
                if !sh.guide || !sh.visible || self.sk.guide_edit == Some((gi, si)) {
                    continue;
                }
                let on = picks.contains(&Pick::Shape(gi, si)) || self.sel == Some(Pick::Group(gi));
                let col = if on { egui::Color32::from_rgb(255, 176, 64) } else { egui::Color32::from_rgb(250, 214, 120) };
                for points in guide_loops(sh) {
                    let px: Vec<egui::Pos2> = points.iter().filter_map(|p| self.cam.project(rect, *p)).collect();
                    if px.len() < 2 {
                        continue;
                    }
                    if on {
                        painter.add(egui::Shape::line(px.clone(), egui::Stroke::new(5.0, egui::Color32::from_black_alpha(90))));
                    }
                    painter.add(egui::Shape::line(px, egui::Stroke::new(if on { 2.6 } else { 1.8 }, col)));
                }
            }
        }
    }

    /// The sketch line close to a point on the screen.
    pub(crate) fn guide_at(&self, rect: egui::Rect, p: egui::Pos2) -> Option<(usize, usize)> {
        let mut best: Option<(f32, (usize, usize))> = None;
        let mut area_best: Option<(f32, (usize, usize))> = None;
        for (gi, g) in self.scene.groups.iter().enumerate() {
            if !g.visible {
                continue;
            }
            for (si, sh) in g.shapes.iter().enumerate() {
                if !sh.guide || !sh.visible {
                    continue;
                }
                let loops: Vec<Vec<_>> =
                    guide_loops(sh).iter().map(|points| points.iter().filter_map(|q| self.cam.project(rect, *q)).collect()).collect();
                for points in &loops {
                    for w in points.windows(2) {
                        let d = seg_dist(p, w[0], w[1]);
                        if d <= PICK_PX && best.is_none_or(|b| d < b.0) {
                            best = Some((d, (gi, si)));
                        }
                    }
                }
                // Reuse the projected outline for the inside test. Holes are not
                // selectable faces, while their edges still belong to this sketch.
                if curve::settings(&sh.params).0
                    && let Some(outer) = loops.first().filter(|outer| outer.len() >= 3)
                    && inside(p, outer)
                    && !loops.iter().skip(1).any(|hole| inside(p, hole))
                {
                    let a = area(outer);
                    if area_best.is_none_or(|b| a < b.0) {
                        area_best = Some((a, (gi, si)));
                    }
                }
            }
        }
        best.or(area_best).map(|b| b.1)
    }
}

/// Whether a screen point is inside a polygon (even-odd rule).
fn inside(p: egui::Pos2, poly: &[egui::Pos2]) -> bool {
    let mut c = false;
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// Area of a polygon on the screen (shoelace).
fn area(poly: &[egui::Pos2]) -> f32 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        s += a.x * b.y - b.x * a.y;
    }
    s.abs() * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MAX_OUTLINE, Material, Op};
    use crate::sketch_saved::loop_normal;

    fn save_profile(app: &mut App, frame: Frame, points: Vec<[f32; 2]>) -> (usize, usize) {
        app.set_sketch_tool(true);
        app.sk.frame = frame;
        app.sketch_done(crate::sketch_ui::Finished::Profile(points));
        assert!(app.sketch_confirm(&egui::Context::default()));
        match app.sel {
            Some(Pick::Shape(g, s)) => (g, s),
            _ => panic!("saved sketch selected"),
        }
    }

    #[test]
    fn saved_sketch_edit_is_a_cancelable_draft_and_preserves_identity() {
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        let (g, s) = save_profile(&mut app, Frame::horizontal(15.0), sketch::rect([0.0, 0.0], [30.0, 20.0]));
        let original = app.scene.shape_mut(g, s).expect("guide");
        original.name = "Bracket outline".into();
        original.uid = 42;
        original.source = "drawing".into();
        original.material.color = [0.2, 0.4, 0.8];
        let before = app.scene.clone();
        assert!(app.edit_saved_sketch(g, s));
        app.sk.profile = sketch::rect([0.0, 0.0], [50.0, 40.0]);
        assert_eq!(app.scene, before, "editing does not mutate the saved source");
        app.set_sketch_tool(false);
        assert_eq!(app.scene, before, "cancel leaves the original sketch intact");
        assert!(app.edit_saved_sketch(g, s));
        app.sk.profile = sketch::rect([0.0, 0.0], [50.0, 40.0]);
        app.sk.dimension = None;
        assert!(app.sketch_confirm(&egui::Context::default()));
        assert_eq!(app.scene.shape_count(), 1);
        let edited = app.scene.shape(g, s).expect("edited");
        let original = before.shape(g, s).expect("original");
        assert_eq!((&edited.name, edited.uid, edited.material, &edited.source), (&original.name, 42, original.material, &original.source));
        let (frame, profile) = app.curve_profile(g, s).expect("profile");
        assert_eq!(frame.origin, [0.0, 0.0, 15.0]);
        assert_eq!(sketch::rectangle_size(&profile), Some([50.0, 40.0]));
    }

    #[test]
    fn loft_uses_two_saved_parallel_sketches_at_their_actual_positions() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        let base = save_profile(&mut app, Frame { origin: [5.0, 7.0, 12.0], ..Frame::floor() }, sketch::rect([-20.0, -20.0], [20.0, 20.0]));
        let top = save_profile(&mut app, Frame { origin: [13.0, 2.0, 52.0], ..Frame::floor() }, sketch::rect([-10.0, -10.0], [10.0, 10.0]));
        let sources = app.scene.clone();
        app.sel = Some(Pick::Shape(base.0, base.1));
        assert!(app.make_from_selected_line(&ctx, Make::Loft));
        assert!(app.use_secondary_sketch(top.0, top.1));
        assert_eq!(app.sk.height, 40.0);
        assert!(!app.sk.flip);
        assert_eq!(sketch::bbox(&app.sk.profile2), Some(([-2.0, -15.0], [18.0, 5.0])));
        assert!(app.sketch_confirm(&ctx));
        assert!(!app.sk.on);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("loft selected") };
        let loft = app.scene.shape(g, s).expect("loft");
        assert_eq!(loft.kind, Kind::Loft);
        assert_eq!(loft.pos, [5.0, 7.0, 32.0]);
        assert_eq!(loft.size[0], 40.0);
        assert_eq!(app.scene.shape(base.0, base.1), sources.shape(base.0, base.1));
        assert_eq!(app.scene.shape(top.0, top.1), sources.shape(top.0, top.1));
    }

    #[test]
    fn loft_rejects_nonparallel_saved_sketch_without_changing_the_preview() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        let base = save_profile(&mut app, Frame::floor(), sketch::rect([0.0, 0.0], [20.0, 20.0]));
        let side = save_profile(&mut app, Frame::front(), sketch::rect([0.0, 0.0], [10.0, 20.0]));
        app.sel = Some(Pick::Shape(base.0, base.1));
        assert!(app.make_from_selected_line(&ctx, Make::Loft));
        let before = app.scene.clone();
        assert!(!app.use_secondary_sketch(side.0, side.1));
        assert_eq!(app.scene, before);
        assert!(app.sk.msg.contains("parallel"));
        assert!(app.sk.secondary_sketch.is_none());
    }

    #[test]
    fn profile_sweep_uses_a_saved_open_path_and_forgets_an_old_path() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        let profile = save_profile(&mut app, Frame::floor(), sketch::rect([-4.0, -3.0], [4.0, 3.0]));
        app.set_sketch_tool(true);
        app.sk.pframe = Frame::front();
        assert!(app.keep_guide(&[[0.0, 0.0], [0.0, 20.0], [10.0, 30.0]], false));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("path selected") };
        app.set_sketch_tool(false);
        assert!(app.curve_profile(g, s).is_none(), "an open three-point path is not a closed profile");
        let sources = app.scene.clone();
        app.sel = Some(Pick::Shape(profile.0, profile.1));
        app.sk.path = vec![[100.0, 0.0, 0.0], [200.0, 0.0, 0.0]];
        assert!(app.make_from_selected_line(&ctx, Make::Sweep));
        assert_eq!(app.sk.stage, Stage::Ready);
        assert!(app.sk.path.is_empty(), "a new operation never reuses an unrelated path");
        assert!(app.use_secondary_sketch(g, s));
        assert_eq!(app.sk.path, vec![[0.0, 0.0, 0.0], [0.0, 0.0, 20.0], [10.0, 0.0, 30.0]]);
        assert!(!app.sk.sweep_round);
        assert!(app.sketch_confirm(&ctx));
        assert!(!app.sk.on);
        let Some(Pick::Shape(sg, si)) = app.sel else { panic!("sweep selected") };
        assert_eq!(app.scene.shape(sg, si).expect("sweep").kind, Kind::Sweep);
        assert_eq!(app.scene.shape(g, s), sources.shape(g, s));
        assert_eq!(app.scene.shape(profile.0, profile.1), sources.shape(profile.0, profile.1));
    }

    #[test]
    fn separate_preview_switches_add_and_cut_without_changing_its_source() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let source = save_profile(&mut app, Frame::floor(), sketch::rect([0.0, 0.0], [20.0, 20.0]));
        let before = app.scene.shape(source.0, source.1).cloned();
        assert!(app.make_from_selected_line(&ctx, Make::Extrude));
        let (g, s) = app.sk.live.expect("preview");
        app.new_op = Op::Subtract;
        app.live_update();
        assert_eq!(app.scene.shape(g, s).expect("cut preview").op, Op::Subtract);
        app.new_op = Op::Add;
        app.live_update();
        assert_eq!(app.scene.shape(g, s).expect("add preview").op, Op::Add);
        assert_eq!(app.scene.shape(source.0, source.1), before.as_ref());
    }

    #[test]
    fn a_sketch_line_is_not_part_of_the_solid() {
        let mut sh = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        assert!(sh.solid());
        sh.guide = true;
        assert!(!sh.solid());
        let back: Shape = serde_json::from_str(&serde_json::to_string(&sh).unwrap_or_default())
            .unwrap_or_else(|_| Shape::new(Kind::Box, [0.0; 3], Material::default()));
        assert!(back.guide, "the flag is saved");
        let pts = guide_points(&Shape {
            path: vec![[0.0, 0.0, 0.0, 1.0], [10.0, 0.0, 0.0, 1.0], [10.0, 10.0, 0.0, 1.0]],
            params: [1.0, 0.0, 0.0, 1.0],
            ..sh
        });
        assert!(pts.len() >= 4 && pts.first() == pts.last(), "a loop is closed again");
        assert!(seg_dist(egui::pos2(5.0, 3.0), egui::pos2(0.0, 0.0), egui::pos2(10.0, 0.0)) - 3.0 < 1e-4);
    }

    #[test]
    fn a_sketch_line_adds_nothing_to_the_field() {
        let mut scene = crate::model::Scene::default();
        scene.groups.clear();
        let mut sh = Shape::new(Kind::Box, [0.0; 3], Material::default());
        sh.guide = true;
        let mut g = crate::model::Group::new("g");
        g.shapes.push(sh);
        scene.groups.push(g);
        let none = crate::sdf::Field::new(&scene).sample([0.0, 0.0, 0.0]).nearest;
        assert!(none.is_none(), "a sketch line is nobody's nearest shape: {none:?}");
        scene.groups[0].shapes[0].guide = false;
        let some = crate::sdf::Field::new(&scene).sample([0.0, 0.0, 0.0]).nearest;
        assert_eq!(some, Some((0, 0)), "the same box as a solid is");
    }

    #[test]
    fn a_point_inside_a_loop_is_found() {
        let sq = [egui::pos2(0.0, 0.0), egui::pos2(10.0, 0.0), egui::pos2(10.0, 10.0), egui::pos2(0.0, 10.0)];
        assert!(inside(egui::pos2(5.0, 5.0), &sq));
        assert!(!inside(egui::pos2(15.0, 5.0), &sq));
        assert!((area(&sq) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn confirmed_sketch_retains_all_vertices_and_plane_after_reload() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        let frame = Frame::oriented([20.0, -30.0, 40.0], [70.0, 15.0, 22.0]).expect("frame");
        let profile = sketch::circle([25.0, 10.0], 12.0, MAX_OUTLINE);
        app.sk.frame = frame;
        app.sketch_done(crate::sketch_ui::Finished::Profile(profile.clone()));
        assert_eq!(app.sk.stage, Stage::Ready);
        assert!(app.sketch_confirm(&ctx));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("saved sketch selected") };
        assert_eq!(app.scene.shape(g, s).expect("guide").path.len(), MAX_OUTLINE);
        assert!(!app.sk.on);
        app.scene = serde_json::from_str(&serde_json::to_string(&app.scene).expect("serialize")).expect("reload");
        app.scene.sanitize();
        let (restored, points) = app.curve_profile(g, s).expect("profile");
        assert_eq!(points.len(), MAX_OUTLINE);
        assert_eq!(restored.origin, frame.origin);
        for (a, b) in profile.iter().zip(&points) {
            assert!((a[0] - b[0]).abs() < 0.001 && (a[1] - b[1]).abs() < 0.001);
        }
        let guide = app.scene.shape(g, s).expect("guide");
        assert_eq!(guide.params[1], 0.0, "the sampled outline is not smoothed again");
        assert_eq!(guide_points(guide).len(), MAX_OUTLINE + 1);
    }

    #[test]
    fn a_saved_front_sketch_revolves_about_its_original_offset_axis() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        let frame = Frame { origin: [50.0, -25.0, 40.0], ..Frame::front() };
        app.sk.frame = frame;
        app.sketch_done(crate::sketch_ui::Finished::Profile(sketch::rect([0.0, 0.0], [10.0, 20.0])));
        assert!(app.sketch_confirm(&ctx));
        assert!(app.make_from_selected_line(&ctx, Make::Revolve));
        let (g, s) = app.sk.live.expect("revolve preview");
        let shape = app.scene.shape(g, s).expect("revolve");
        assert_eq!(shape.kind, Kind::Revolve);
        for (actual, expected) in shape.pos.iter().zip([50.0, -25.0, 50.0]) {
            assert!((actual - expected).abs() < 0.001, "axis and height must not be recentered");
        }
        let (lo, hi) = sketch::bbox(&shape.outline).expect("bounds");
        assert_eq!((lo, hi), ([0.0, -10.0], [10.0, 10.0]));
        assert_eq!(shape.blend, 0.0, "a solid command keeps its exact boundary");
    }

    #[test]
    fn old_sketches_keep_their_origin_and_nonplanar_curves_are_rejected() {
        let mut app = App::new(true, None);
        let mut sh = Shape::new(Kind::Curve, [20.0, 30.0, 40.0], Material::default());
        sh.guide = true;
        sh.params = [1.0, 0.0, 0.0, 1.0];
        sh.path = vec![[0.0, 0.0, 0.0, 0.6], [10.0, 0.0, 0.0, 0.6], [10.0, 0.0, 20.0, 0.6], [0.0, 0.0, 20.0, 0.6]];
        app.scene.groups[0].shapes.push(sh);
        let s = app.scene.groups[0].shapes.len() - 1;
        let (frame, profile) = app.curve_profile(0, s).expect("legacy profile");
        assert_eq!(frame.origin, [20.0, 30.0, 40.0]);
        assert!(sketch::revolve(&frame, &profile).is_ok());
        app.scene.groups[0].shapes[s].path[2][1] = 5.0;
        assert!(app.curve_profile(0, s).is_none(), "do not silently flatten a 3D curve");
    }

    #[test]
    fn saved_holes_survive_reload_and_both_sided_extrusion() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups.clear();
        app.set_sketch_tool(true);
        app.sk.frame = Frame::horizontal(12.0);
        app.sketch_done(crate::sketch_ui::Finished::Profile(sketch::rect([-20.0, -20.0], [20.0, 20.0])));
        app.sk.holes = vec![sketch::rect([-5.0, -5.0], [5.0, 5.0])];
        assert!(app.sketch_confirm(&ctx));
        let guide = app.sel;
        app.scene = serde_json::from_str(&serde_json::to_string(&app.scene).expect("serialize")).expect("reload");
        app.scene.sanitize();
        assert!(app.make_from_selected_line(&ctx, Make::Extrude));
        assert_eq!(app.sk.holes.len(), 1);
        app.sk.both = true;
        app.sk.height = 20.0;
        assert!(app.sketch_confirm(&ctx));
        assert!(!app.sk.on);
        assert!(guide.is_some_and(|p| match p {
            Pick::Shape(g, s) => app.scene.shape(g, s).is_some_and(|sh| sh.guide),
            _ => false,
        }));
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("solid selected") };
        assert_eq!(s, 0, "select the solid rather than the last hole");
        assert_eq!(app.scene.groups[g].shapes.len(), 2);
        for shape in &app.scene.groups[g].shapes {
            assert!((shape.pos[2] - 12.0).abs() < 0.001, "both the solid and hole must straddle the plane");
        }
        let field = crate::sdf::Field::new(&app.scene);
        assert!(field.distance([0.0, 0.0, 4.0]) > 0.0 && field.distance([0.0, 0.0, 20.0]) > 0.0);
        assert!(field.distance([10.0, 0.0, 4.0]) < 0.0 && field.distance([10.0, 0.0, 20.0]) < 0.0);
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
