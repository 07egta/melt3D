//! A solid command previews a saved sketch with a height handle and a small card.
//! Accept keeps the preview (one undo step); discard restores the source selection.
//! Internal legacy callers can also create a live preview from a drawing gesture.
//!
//!  - Extrude / Loft: drag the arrow over the top face (or type the height). Dragging below the
//!    plane flips the solid. Shift = 10 mm, Ctrl = 0.1 mm.
//!  - Sweep: draw the path first; a round tube follows it. The card sets its radius.
//!  - Enter or right-click accepts, Esc discards.

use egui::{Color32, Painter, Pos2, Rect, Shape as Gfx, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::curve_tool::Input;
use crate::model::{Kind, MAX_SHAPES, Material, Op, Pick, Shape};
use crate::sdf::{self, V3};
use crate::sketch::{self, Built, SketchError};
use crate::sketch_ui::{COLORS, LoftTop, Make, Stage, step_for};
use crate::style;

/// Pixels within which the mouse grabs the height handle.
const GRAB_PX: f32 = 16.0;
/// Radius of the round profile a new sweep starts with (mm, at scale 1).
const SWEEP_RADIUS: f32 = 4.0;

/// Where on the line `line_o + t * line_d` the viewing ray `ray_o + s * ray_d` comes closest.
/// None when the ray runs along the line or the numbers are not finite.
pub fn along_line(ray_o: V3, ray_d: V3, line_o: V3, line_d: V3) -> Option<f32> {
    let w = sdf::sub(ray_o, line_o);
    let (a, b, c) = (sdf::dot(ray_d, ray_d), sdf::dot(ray_d, line_d), sdf::dot(line_d, line_d));
    let (d, f) = (sdf::dot(ray_d, w), sdf::dot(line_d, w));
    let den = a * c - b * b;
    if !den.is_finite() || den.abs() < 1e-9 * (a * c).abs().max(1e-12) {
        return None;
    }
    let t = (a * f - b * d) / den;
    t.is_finite().then_some(t)
}

/// The height and the side for a handle that sits `t` mm along the plane's normal: snapped to
/// `step`, at least 0.1 mm, at most 2000 mm; below the plane the solid is flipped.
pub fn height_from(t: f32, step: f32) -> Option<(f32, bool)> {
    if !t.is_finite() {
        return None;
    }
    let step = if step.is_finite() && step > 0.0 { step } else { 1.0 };
    let snapped = (t / step).round() * step;
    Some((snapped.abs().clamp(0.1, 2000.0), t < 0.0))
}

fn near(a: Pos2, b: Pos2) -> bool {
    (a - b).length() <= GRAB_PX
}

impl App {
    // ----- the numbers -----

    /// The shape the card is set to make, without touching the scene.
    pub(crate) fn build_shape(&self) -> Result<Built, SketchError> {
        let st = &self.sk;
        match st.make {
            Make::Extrude => {
                let mut b = sketch::extrude(&st.pframe, &st.profile, st.height, st.flip)?;
                if st.both {
                    // half each way: the middle of the solid sits on the plane
                    b.pos = st.pframe.to_world(sketch::centre(&st.profile));
                }
                Ok(b)
            }
            Make::Revolve => sketch::revolve(&st.pframe, &st.profile),
            Make::Loft => {
                let top = match st.loft_top {
                    LoftTop::Scaled => sketch::scaled(&st.profile, st.loft_scale),
                    LoftTop::Drawn => st.profile2.clone(),
                };
                sketch::loft(&st.pframe, &st.profile, &top, st.height, st.flip)
            }
            Make::Sweep => sketch::sweep(&st.profile, &st.path, st.sweep_scale),
        }
    }

    /// The live solid still stands where the sketch left it.
    pub(crate) fn live_alive(&self) -> bool {
        self.sk.live.is_some_and(|(g, s)| {
            self.scene.shape(g, s).is_some_and(|sh| matches!(sh.kind, Kind::Extrude | Kind::Loft | Kind::Sweep | Kind::Revolve))
        })
    }

    /// Called every frame: a live solid that was undone, deleted or replaced ends the live state.
    pub(crate) fn live_validate(&mut self) {
        let lost = (self.sk.live.is_some() && !self.live_alive()) || (self.sk.stage == Stage::Shape && self.sk.live.is_none());
        if lost {
            self.sk.reset();
        }
    }

    // ----- making, changing, ending -----

    /// The outline is closed (or the sweep path is drawn): put the solid into the scene.
    pub(crate) fn live_begin(&mut self) -> bool {
        if self.scene.shape_count() >= MAX_SHAPES {
            self.sk.msg = format!("Up to {MAX_SHAPES} shapes");
            return false;
        }
        let built = match self.build_shape() {
            Ok(b) => b,
            Err(e) => {
                self.sk.msg = e.message();
                return false;
            }
        };
        let make = self.sk.make;
        let groups_before = self.scene.groups.len();
        let prev = self.sel;
        let Some(g) = self.target_group() else { return false };
        let mut color = COLORS[self.scene.shape_count() % COLORS.len()];
        if self.new_op == Op::Subtract {
            color = [0.9, 0.35, 0.4];
        }
        let mut sh = Shape::new(make.kind(), built.pos, Material { color, ..Material::default() });
        sketch::apply(&built, &mut sh);
        sh.name = make.label().to_string();
        sh.op = self.new_op;
        sh.blend = if self.sk.separate || self.solid_mode { 0.0 } else { 2.0 };
        let created = self.scene.groups.len() > groups_before;
        let Some(group) = self.scene.groups.get_mut(g) else { return false };
        group.shapes.push(sh);
        let idx = group.shapes.len() - 1;
        self.sk.live = Some((g, idx));
        self.sk.live_group = created;
        self.sk.live_sel = prev;
        self.sk.handle_drag = false;
        self.sel = Some(Pick::Shape(g, idx));
        self.sk.stage = Stage::Shape;
        self.sk.msg.clear();
        true
    }

    /// Rebuild the live solid from the card's numbers (the previous one stays when that fails).
    pub(crate) fn live_update(&mut self) {
        let Some((g, s)) = self.sk.live else { return };
        if !self.live_alive() {
            self.sk.reset();
            return;
        }
        match self.build_shape() {
            Ok(b) => {
                let label = self.sk.make.label();
                let op = self.new_op;
                let color = if op == Op::Subtract { [0.9, 0.35, 0.4] } else { COLORS[self.scene.shape_count().saturating_sub(1) % COLORS.len()] };
                if let Some(sh) = self.scene.shape_mut(g, s) {
                    sketch::apply(&b, sh);
                    if sh.op != op {
                        sh.op = op;
                        sh.material.color = color;
                    }
                    if Make::ALL.iter().any(|m| m.label() == sh.name) {
                        sh.name = label.to_string();
                    }
                }
                self.sk.msg.clear();
            }
            Err(e) => self.sk.msg = e.message(),
        }
    }

    /// Keep the solid and go on sketching.
    pub(crate) fn live_accept(&mut self, ctx: &egui::Context) {
        if self.sk.live.is_none() {
            return;
        }
        if !self.live_alive() {
            self.sk.reset();
            return;
        }
        if let Err(e) = self.build_shape() {
            self.sk.msg = e.message();
            return;
        }
        self.live_update();
        let make = self.sk.make;
        let separate = self.sk.separate;
        self.sk.reset();
        if separate {
            self.sk.on = false;
        } else if make == Make::Sweep {
            self.sk.path.clear();
            self.sk.stage = Stage::SweepPath;
        }
        self.sk.frame = self.sketch_plane();
        self.say(ctx, format!("Created {} (use Edit Sketch to change the outline)", make.label()));
    }

    /// Remove the live solid again: the scene is as it was before the outline closed.
    pub(crate) fn live_discard(&mut self) {
        if let Some((g, s)) = self.sk.live {
            if self.live_alive() {
                if let Some(group) = self.scene.groups.get_mut(g)
                    && s < group.shapes.len()
                {
                    group.shapes.remove(s);
                }
                if self.sk.live_group && g + 1 == self.scene.groups.len() && self.scene.groups.get(g).is_some_and(|gr| gr.shapes.is_empty()) {
                    self.scene.groups.pop();
                }
            }
            self.sel = self.sk.live_sel;
            self.fix_selection();
        }
        let make = self.sk.make;
        let separate = self.sk.separate;
        self.sk.reset();
        self.sk.path.clear();
        if separate {
            self.sk.on = false;
        } else if make == Make::Sweep {
            self.sk.stage = Stage::SweepPath;
        }
    }

    /// Take the live solid back and show the "Ready" card for the same outline (the way to add
    /// holes). Does nothing when no solid is live.
    pub(crate) fn live_to_ready(&mut self) {
        if self.sk.live.is_none() {
            return;
        }
        let (profile, pframe, separate) = (self.sk.profile.clone(), self.sk.pframe, self.sk.separate);
        self.live_discard();
        self.sk.profile = profile;
        self.sk.pframe = pframe;
        self.sk.frame = pframe;
        self.sk.make = Make::Extrude;
        self.sk.on = true;
        self.sk.separate = separate;
        self.sk.stage = Stage::Ready;
    }

    /// A strip button was pressed: make `m` the way of making (changing the live solid in place
    /// when it is a prism and so is `m`).
    pub(crate) fn sketch_choose_make(&mut self, m: Make) {
        if self.sk.live.is_some() {
            if matches!(m, Make::Extrude | Make::Loft) && matches!(self.sk.make, Make::Extrude | Make::Loft) {
                self.sk.make = m;
                self.live_update();
                return;
            }
            self.live_discard();
        }
        self.sk.make = m;
        self.sk.keep_sketch = false;
        self.sk.msg.clear();
        match (m, self.sk.stage) {
            (Make::Sweep, Stage::Draw | Stage::SweepPath) if self.sweep_from_curve() => {}
            (Make::Sweep, Stage::Draw | Stage::SweepPath) => {
                self.sk.clear_drawing();
                self.sk.profile.clear();
                self.sk.path.clear();
                self.sk.stage = Stage::SweepPath;
                self.sk.frame = self.sketch_plane();
            }
            (_, Stage::SweepPath) => {
                self.sk.clear_drawing();
                self.sk.stage = Stage::Draw;
            }
            _ => {}
        }
    }

    /// With a curve selected, sweep a round tube along it right away.
    fn sweep_from_curve(&mut self) -> bool {
        let Some(Pick::Shape(g, s)) = self.sel else { return false };
        let Some(sh) = self.scene.shape(g, s).filter(|sh| sh.kind == Kind::Curve) else { return false };
        let path: Vec<V3> = sh.path.iter().map(|n| crate::curve_tool::to_world(sh, [n[0], n[1], n[2]])).collect();
        self.sk.clear_drawing();
        self.sk.path = path;
        self.sk.sweep_round = true;
        self.sk.profile = sketch::circle([0.0, 0.0], SWEEP_RADIUS, sketch::CIRCLE_POINTS);
        self.sk.pframe = self.sk.frame;
        self.sk.make = Make::Sweep;
        if self.live_begin() {
            true
        } else {
            self.sk.profile.clear();
            self.sk.path.clear();
            false
        }
    }

    /// A sweep path was drawn first: a round tube follows it.
    pub(crate) fn sweep_path_done(&mut self) {
        self.sk.make = Make::Sweep;
        self.sk.sweep_round = true;
        self.sk.profile = sketch::circle([0.0, 0.0], SWEEP_RADIUS, sketch::CIRCLE_POINTS);
        self.sk.pframe = self.sk.frame;
        if !self.live_begin() {
            self.sk.profile.clear();
            self.sk.path.clear();
            self.sk.stage = Stage::SweepPath;
        }
    }

    // ----- the handle -----

    /// The foot and the tip of the height arrow (world), for a prism being shaped.
    fn handle_world(&self) -> Option<(V3, V3)> {
        if self.sk.live.is_none()
            || self.sk.stage != Stage::Shape
            || !matches!(self.sk.make, Make::Extrude | Make::Loft)
            || (self.sk.make == Make::Loft && self.sk.secondary_sketch.is_some())
        {
            return None;
        }
        let f = &self.sk.pframe;
        if self.sk.profile.is_empty() {
            return None;
        }
        let base = f.to_world(sketch::centre(&self.sk.profile));
        let h = if self.sk.height.is_finite() { self.sk.height.clamp(0.1, 2000.0) } else { 10.0 };
        let reach = if self.sk.both && self.sk.make == Make::Extrude { h * 0.5 } else { h };
        let tip = sdf::add(base, sdf::mul(f.normal(), if self.sk.flip { -reach } else { reach }));
        (base.iter().chain(tip.iter()).all(|v| v.is_finite())).then_some((base, tip))
    }

    /// The mouse in the shaping stage: the height handle, and a right click that accepts.
    pub(crate) fn live_input(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, pos: Option<Pos2>, mods: egui::Modifiers) -> Input {
        let mut out = Input::default();
        let step = step_for(mods);
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let tip_px = self.handle_world().and_then(|(_, tip)| self.cam.project(rect, tip));
        if resp.drag_started_by(egui::PointerButton::Primary)
            && !mods.alt
            && let (Some(t), Some(o)) = (tip_px, ui.input(|i| i.pointer.press_origin()).or(pos))
            && near(t, o)
        {
            self.sk.handle_drag = true;
        }
        if self.sk.handle_drag {
            out.taken = true;
            out.busy = true;
            if !primary_down || resp.drag_stopped() {
                self.sk.handle_drag = false;
            } else if let (Some(p), Some((base, _))) = (pos, self.handle_world()) {
                let (o, d) = self.cam.ray_at(rect, p);
                let n = self.sk.pframe.normal();
                let both = self.sk.both && self.sk.make == Make::Extrude;
                if let Some((h, flip)) =
                    along_line(o, d, base, n).and_then(|t| height_from(t, step)).map(|(h, f)| (if both { (h * 2.0).min(2000.0) } else { h }, f))
                    && (h != self.sk.height || flip != self.sk.flip)
                {
                    self.sk.height = h;
                    self.sk.flip = flip;
                    self.live_update();
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if let (Some(t), Some(m)) = (tip_px, pos)
            && near(t, m)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if resp.secondary_clicked() && resp.hovered() && pos.is_some() && !mods.alt {
            self.sketch_confirm(ui.ctx());
        }
        // nothing else to draw: clicks must not pick things behind the card
        if pos.is_some()
            && (resp.clicked_by(egui::PointerButton::Primary)
                || resp.drag_started_by(egui::PointerButton::Primary)
                || resp.dragged_by(egui::PointerButton::Primary)
                || resp.secondary_clicked())
        {
            out.taken = true;
        }
        out
    }

    /// The outline on its plane and the height arrow.
    pub(crate) fn paint_live(&self, p: &Painter, rect: Rect) {
        let st = &self.sk;
        let f = &st.pframe;
        let pts: Vec<V3> = st.profile.iter().map(|q| f.to_world(*q)).collect();
        if st.make == Make::Sweep {
            self.paint_saved_path(p, rect);
        } else {
            self.paint_world_poly(p, rect, &pts, true, style::ACCENT, 1.8);
        }
        let Some((base, tip)) = self.handle_world() else { return };
        let (Some(a), Some(b)) = (self.cam.project(rect, base), self.cam.project(rect, tip)) else { return };
        let hot = st.handle_drag || st.mouse.is_some_and(|m| near(b, m));
        let col = if hot { Color32::WHITE } else { style::ACCENT };
        p.line_segment([a, b], Stroke::new(2.0, col.gamma_multiply(0.8)));
        p.circle_filled(b, if hot { 9.0 } else { 7.5 }, style::ACCENT_SOFT);
        p.circle_stroke(b, if hot { 9.0 } else { 7.5 }, Stroke::new(1.6, col));
        let dir = (b - a).normalized();
        let dir = if dir.x.is_finite() && dir.y.is_finite() { dir } else { vec2(0.0, -1.0) };
        let side = vec2(-dir.y, dir.x);
        let c = b + dir * 2.0;
        p.add(Gfx::convex_polygon(vec![c + dir * 5.0, c - dir * 3.0 + side * 4.5, c - dir * 3.0 - side * 4.5], col, Stroke::NONE));
        let h = if st.height.is_finite() { st.height } else { 0.0 };
        let label = format!("{} mm", if (h - h.round()).abs() < 0.05 { format!("{}", h.round()) } else { format!("{h:.1}") });
        p.text(pos2(b.x + 14.0, b.y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), col);
    }

    // ----- the card -----

    /// Controls of the live solid: numbers and icons (the words are in the tips).
    pub(crate) fn shape_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let make = self.sk.make;
        let mut changed = false;
        let (mut accept, mut discard, mut hole, mut top) = (false, false, false, false);
        ui.label(egui::RichText::new(make.label()).strong().color(style::ACCENT));
        ui.add(
            egui::Label::new(egui::RichText::new("Adjust the preview  ·  Right-click / Enter: confirm  ·  Esc: cancel").size(11.0).color(style::DIM))
                .wrap(),
        );
        changed |= self.saved_sketch_inputs(ui);
        let saved_loft = make == Make::Loft && self.sk.secondary_sketch.is_some();
        if saved_loft {
            ui.label(format!("{:.2} mm between sketch planes", self.sk.height));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if matches!(make, Make::Extrude | Make::Loft) && !saved_loft {
                let (rc, _) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::hover());
                crate::sketch_icons::height(ui.painter(), rc, style::TEXT);
                let mut h = if self.sk.height.is_finite() { self.sk.height } else { 10.0 };
                if ui
                    .add(egui::DragValue::new(&mut h).speed(0.5).range(0.1..=2000.0).suffix(" mm"))
                    .on_hover_text("Height. Drag the arrow in the view, or type a number")
                    .changed()
                {
                    self.sk.height = h;
                    changed = true;
                }
            }
            if make == Make::Loft && self.sk.loft_top == LoftTop::Scaled {
                let mut s = if self.sk.loft_scale.is_finite() { self.sk.loft_scale } else { 0.5 };
                if ui
                    .add(egui::DragValue::new(&mut s).speed(0.01).range(0.1..=3.0).prefix("x "))
                    .on_hover_text("Top size (1 = as big as the bottom)")
                    .changed()
                {
                    self.sk.loft_scale = s;
                    changed = true;
                }
            }
            if make == Make::Sweep && self.sk.sweep_round {
                let mut r = if self.sk.sweep_scale.is_finite() { self.sk.sweep_scale * SWEEP_RADIUS } else { SWEEP_RADIUS };
                if ui
                    .add(egui::DragValue::new(&mut r).speed(0.1).range(0.4..=SWEEP_RADIUS * 4.0).prefix("r ").suffix(" mm"))
                    .on_hover_text("Radius of the tube")
                    .changed()
                {
                    self.sk.sweep_scale = r / SWEEP_RADIUS;
                    changed = true;
                }
            }
            if make == Make::Sweep && !self.sk.sweep_round {
                ui.label("Profile scale");
                if ui.add(egui::DragValue::new(&mut self.sk.sweep_scale).speed(0.01).range(0.2..=4.0).suffix(" ×")).changed() {
                    changed = true;
                }
            }
            if make == Make::Loft && !self.sk.separate {
                top = style::wide_button(
                    ui,
                    "Top outline",
                    "Draw the top outline yourself",
                    self.sk.loft_top == LoftTop::Drawn,
                    None,
                    crate::sketch_icons::pencil_top,
                )
                .clicked();
            }
            if make == Make::Extrude {
                let both = style::wide_button(
                    ui,
                    "Both sides",
                    "Grow equally to both sides of the plane",
                    self.sk.both,
                    None,
                    crate::sketch_icons::both_sides,
                )
                .clicked();
                if both {
                    self.sk.both = !self.sk.both;
                    changed = true;
                }
                if !self.sk.separate {
                    hole = style::wide_button(ui, "Holes", "Cut holes into this solid", false, None, crate::sketch_icons::hole).clicked();
                }
            }
            accept =
                style::wide_button(ui, "Confirm", "Keep it (Enter or right-click)", false, Some(style::ACCENT), crate::sketch_icons::check).clicked();
            discard = style::wide_button(ui, "Cancel", "Cancel this operation (Esc)", false, None, crate::sketch_icons::cross).clicked();
        });
        if !self.sk.msg.is_empty() {
            ui.label(egui::RichText::new(&self.sk.msg).size(11.5).color(style::CUT));
        }
        if changed {
            self.live_update();
        }
        if top {
            self.sk.loft_top = LoftTop::Drawn;
            self.sk.profile2.clear();
            self.sk.clear_drawing();
            self.sk.stage = Stage::LoftTop;
        } else if hole {
            // the old way: the same outline, with the card that can hold holes
            self.live_to_ready();
        } else if accept {
            self.live_accept(ctx);
        } else if discard {
            self.live_discard();
        }
    }
}
