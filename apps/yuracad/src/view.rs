//! The viewport: camera controls, picking, and the move / rotate / scale handles.

use std::sync::Arc;

use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use crate::app::App;
use crate::camera::Camera;
use crate::model::{Kind, Pick, Scene};
use crate::sdf::{Prepared, V3, add, cross, dot, len, mul, sub};
use crate::style;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    Axis(usize),
    Centre,
}

/// A handle being dragged.
#[derive(Clone, Debug)]
pub struct Drag {
    handle: Handle,
    tool: Tool,
    start: Pos2,
    centre: V3,
    /// The axis in the world, its direction on screen (unit) and its length on screen.
    axis: V3,
    screen_dir: Vec2,
    len_px: f32,
    ring_px: f32,
    mm_px: f32,
    /// (group, shape, position, rotation, scale) when the drag began.
    from: Vec<(usize, usize, V3, V3, V3)>,
}

/// The shapes a pick stands for.
pub fn targets(scene: &Scene, pick: Option<Pick>) -> Vec<(usize, usize)> {
    match pick {
        Some(Pick::Shape(g, s)) if scene.shape(g, s).is_some() => vec![(g, s)],
        Some(Pick::Group(g)) => scene.groups.get(g).map(|gr| (0..gr.shapes.len()).map(|s| (g, s)).collect()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Where the handles sit: the shape, or the middle of a group.
pub fn centre_of(scene: &Scene, pick: Option<Pick>) -> Option<V3> {
    let t = targets(scene, pick);
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for (g, s) in &t {
        let sh = scene.shape(*g, *s)?;
        sum = add(sum, sh.pos);
        n += 1.0;
    }
    (n > 0.0).then(|| mul(sum, 1.0 / n))
}

/// A rough bounding radius of a shape, for framing the view.
pub fn radius_of(s: &crate::model::Shape) -> f32 {
    let m = s.scale[0].max(s.scale[1]).max(s.scale[2]);
    let r = match s.kind {
        Kind::Sphere => s.size[0],
        Kind::Box => len(s.size) * 0.5,
        Kind::Cylinder | Kind::Cone => len([s.size[0], s.size[1] * 0.5, 0.0]),
        Kind::Torus => s.size[0] + s.size[1],
        Kind::Extrude => {
            let w = s.outline.iter().map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt()).fold(0.0_f32, f32::max);
            len([w, s.size[0] * 0.5, 0.0])
        }
    };
    r * m
}

const RING_SEGS: usize = 56;

struct Geom {
    centre: Pos2,
    /// World length of each handle and its end point on screen.
    len_world: f32,
    ends: [Option<Pos2>; 3],
    rings: [Vec<Pos2>; 3],
    axes: [V3; 3],
}

fn ring_points(cam: &Camera, rect: Rect, c: V3, axis: usize, r: f32) -> Vec<Pos2> {
    let (a, b) = match axis {
        0 => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        1 => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        _ => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    };
    (0..=RING_SEGS)
        .filter_map(|i| {
            let t = i as f32 / RING_SEGS as f32 * std::f32::consts::TAU;
            cam.project(rect, add(c, add(mul(a, r * t.cos()), mul(b, r * t.sin()))))
        })
        .collect()
}

fn geom(cam: &Camera, rect: Rect, centre: V3, axes: [V3; 3], tool: Tool) -> Option<Geom> {
    let c = cam.project(rect, centre)?;
    let len_world = 78.0 * cam.mm_per_px(centre, rect.height());
    let ends = [0, 1, 2].map(|i| cam.project(rect, add(centre, mul(axes[i], len_world))));
    let rings = if tool == Tool::Rotate {
        [0, 1, 2].map(|i| ring_points(cam, rect, centre, i, len_world * 0.9))
    } else {
        [Vec::new(), Vec::new(), Vec::new()]
    };
    Some(Geom { centre: c, len_world, ends, rings, axes })
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq().max(1e-6);
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn hit(g: &Geom, tool: Tool, p: Pos2) -> Option<Handle> {
    if (p - g.centre).length() < 9.0 && tool != Tool::Rotate {
        return Some(Handle::Centre);
    }
    let mut best: Option<(f32, usize)> = None;
    for i in 0..3 {
        let d = if tool == Tool::Rotate {
            g.rings[i].windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f32::MAX, f32::min)
        } else {
            match g.ends[i] {
                Some(e) => seg_dist(p, g.centre, e),
                None => f32::MAX,
            }
        };
        if d < 8.0 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| Handle::Axis(i))
}

/// The gizmo's axes: the world's, or for scaling the shape's own.
fn axes_for(scene: &Scene, pick: Option<Pick>, tool: Tool) -> [V3; 3] {
    if tool == Tool::Scale
        && let Some(Pick::Shape(g, s)) = pick
        && let Some(sh) = scene.shape(g, s)
    {
        return Prepared::new(sh).inv;
    }
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

fn rotate_about(p: V3, axis: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    add(add(mul(p, c), mul(cross(axis, p), s)), mul(axis, dot(axis, p) * (1.0 - c)))
}

impl App {
    /// Draw the viewport and handle the mouse. Returns nothing: it edits `self`.
    pub fn viewport(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let data = crate::gpu::pack(&self.scene, &self.cam, [rect.width() * ppp, rect.height() * ppp], self.sel, self.srgb);
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, crate::gpu::Callback { data: Arc::new(data) }));
        let painter = ui.painter_at(rect);

        let mods = ui.input(|i| i.modifiers);
        let pos = resp.hover_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()));

        // The handles
        let centre = centre_of(&self.scene, self.sel);
        let tool = self.tool;
        let axes = axes_for(&self.scene, self.sel, tool);
        let g = if tool != Tool::Select { centre.and_then(|c| geom(&self.cam, rect, c, axes, tool)) } else { None };
        let over_gnomon = pos.is_some_and(|p| self.gnomon_hit(rect, p).is_some());
        let hover_handle = match (&g, pos, &self.drag) {
            (Some(g), Some(p), None) if resp.hovered() && !over_gnomon => hit(g, tool, p),
            _ => None,
        };
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if hover_handle.is_some() || over_gnomon { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default });
        }

        // Camera
        if resp.dragged_by(PointerButton::Middle) || (resp.dragged_by(PointerButton::Primary) && mods.alt && self.drag.is_none()) {
            let d = resp.drag_delta();
            if mods.shift {
                self.cam.pan(d.x, d.y, rect.height());
            } else if mods.ctrl {
                self.cam.zoom(-d.y * 2.0);
            } else {
                self.cam.orbit(d.x, d.y);
            }
        }
        if resp.dragged_by(PointerButton::Secondary) {
            let d = resp.drag_delta();
            self.cam.pan(d.x, d.y, rect.height());
        }
        if resp.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if scroll != 0.0 {
                self.cam.zoom(scroll);
            }
            if pinch != 1.0 {
                self.cam.dist = (self.cam.dist / pinch).clamp(4.0, 4000.0);
            }
        }

        // Dragging a handle
        if resp.drag_started_by(PointerButton::Primary)
            && !mods.alt
            && let (Some(h), Some(g), Some(p), Some(c)) = (hover_handle, &g, pos, centre)
        {
            self.drag = Some(self.begin_drag(h, g, p, c, tool));
        }
        if let Some(d) = self.drag.clone() {
            if resp.dragged_by(PointerButton::Primary)
                && let Some(p) = ui.input(|i| i.pointer.interact_pos())
            {
                self.apply_drag(&d, p, mods.ctrl);
            }
            if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                self.drag = None;
            }
        }

        // Clicks
        if resp.clicked_by(PointerButton::Primary)
            && hover_handle.is_none()
            && let Some(p) = pos
        {
            if let Some(axis) = self.gnomon_hit(rect, p) {
                self.look_along(axis);
            } else {
                self.pick_at(rect, p, mods.shift);
            }
        }
        if resp.secondary_clicked()
            && let Some(p) = pos
        {
            self.pick_at(rect, p, false);
        }
        resp.context_menu(|ui| self.context_menu(ui));
        if resp.double_clicked_by(PointerButton::Primary) && hover_handle.is_none() && self.sel.is_none() {
            self.frame_view(false);
        }

        // Draw the handles over the picture
        if let Some(g) = &g {
            let active = self.drag.as_ref().map(|d| d.handle).or(hover_handle);
            Self::draw_gizmo(&painter, g, tool, active);
        }
        self.draw_gnomon(&painter, rect, pos);
        painter.text(
            rect.left_top() + vec2(12.0, 10.0),
            Align2::LEFT_TOP,
            "右ドラッグ: 移動　中ドラッグ: 回転　ホイール: ズーム　クリック: 選ぶ",
            FontId::proportional(11.5),
            Color32::from_white_alpha(80),
        );
    }

    fn begin_drag(&self, handle: Handle, g: &Geom, mouse: Pos2, centre: V3, tool: Tool) -> Drag {
        let axis_i = if let Handle::Axis(i) = handle { i } else { 0 };
        let axis = g.axes[axis_i];
        let end = g.ends[axis_i].unwrap_or(g.centre + vec2(1.0, 0.0));
        let dir = end - g.centre;
        let len_px = dir.length().max(8.0);
        let screen_dir = if dir.length() > 1e-3 { dir / dir.length() } else { vec2(1.0, 0.0) };
        let ring_px = {
            let pts = &g.rings[axis_i];
            // the ring's apparent radius: its mean distance from the centre
            if pts.is_empty() { 60.0 } else { pts.iter().map(|p| (*p - g.centre).length()).sum::<f32>() / pts.len() as f32 }
        };
        // the direction along the ring where it was grabbed
        let mut tangent = screen_dir;
        if tool == Tool::Rotate {
            let pts = &g.rings[axis_i];
            if let Some((i, _)) = pts.iter().enumerate().min_by(|a, b| (*a.1 - mouse).length().total_cmp(&(*b.1 - mouse).length())) {
                let a = pts[i.saturating_sub(1)];
                let b = pts[(i + 1).min(pts.len() - 1)];
                let t = b - a;
                if t.length() > 1e-3 {
                    tangent = t / t.length();
                }
            }
        }
        let from = targets(&self.scene, self.sel)
            .into_iter()
            .filter_map(|(gi, si)| self.scene.shape(gi, si).map(|s| (gi, si, s.pos, s.rot, s.scale)))
            .collect();
        Drag {
            handle,
            tool,
            start: mouse,
            centre,
            axis,
            screen_dir: if tool == Tool::Rotate { tangent } else { screen_dir },
            len_px,
            ring_px: ring_px.max(20.0),
            mm_px: self.cam.mm_per_px(centre, self.last_view_height.max(1.0)),
            from,
        }
    }

    fn apply_drag(&mut self, d: &Drag, mouse: Pos2, snap: bool) {
        let delta = mouse - d.start;
        let group = matches!(self.sel, Some(Pick::Group(_)));
        let (right, up, _) = self.cam.basis();
        let round = |v: f32, step: f32| if snap { (v / step).round() * step } else { v };
        for &(gi, si, pos, rot, scale) in &d.from {
            let Some(s) = self.scene.shape_mut(gi, si) else { continue };
            match (d.tool, d.handle) {
                (Tool::Move, Handle::Axis(_)) => {
                    let amt = round(delta.dot(d.screen_dir) * d.mm_px * (78.0 / d.len_px), 1.0);
                    s.pos = add(pos, mul(d.axis, amt));
                    if snap {
                        s.pos = [round(s.pos[0], 1.0), round(s.pos[1], 1.0), round(s.pos[2], 1.0)];
                    }
                }
                (Tool::Move, Handle::Centre) => {
                    let off = add(mul(right, delta.x * d.mm_px), mul(up, -delta.y * d.mm_px));
                    s.pos = add(pos, off);
                    if snap {
                        s.pos = [round(s.pos[0], 1.0), round(s.pos[1], 1.0), round(s.pos[2], 1.0)];
                    }
                }
                (Tool::Rotate, Handle::Axis(i)) => {
                    let deg = round(delta.dot(d.screen_dir) / d.ring_px * 57.2958, 15.0);
                    s.rot[i] = (rot[i] + deg).rem_euclid(360.0);
                    if group {
                        let around = sub(pos, d.centre);
                        s.pos = add(d.centre, rotate_about(around, d.axis, deg));
                    }
                }
                (Tool::Scale, Handle::Axis(i)) if !group => {
                    let f = (1.0 + delta.dot(d.screen_dir) / d.len_px).max(0.05);
                    s.scale[i] = round(scale[i] * f, 0.05).clamp(0.01, 100.0);
                }
                (Tool::Scale, _) => {
                    let f = ((delta.x - delta.y) / 160.0).exp();
                    for (dst, src) in s.scale.iter_mut().zip(scale) {
                        *dst = (src * f).clamp(0.01, 100.0);
                    }
                    if group {
                        s.pos = add(d.centre, mul(sub(pos, d.centre), f));
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_gizmo(p: &egui::Painter, g: &Geom, tool: Tool, active: Option<Handle>) {
        let c = g.centre;
        for i in 0..3 {
            let on = active == Some(Handle::Axis(i));
            let col = if on { Color32::WHITE } else { style::AXIS[i] };
            let w = if on { 3.5 } else { 2.5 };
            match tool {
                Tool::Rotate => {
                    if g.rings[i].len() > 1 {
                        p.add(Shape::line(g.rings[i].clone(), Stroke::new(w, col.gamma_multiply(if on { 1.0 } else { 0.85 }))));
                    }
                }
                _ => {
                    let Some(e) = g.ends[i] else { continue };
                    p.line_segment([c, e], Stroke::new(w, col));
                    let d = (e - c).normalized();
                    if tool == Tool::Move {
                        let n = vec2(-d.y, d.x);
                        p.add(Shape::convex_polygon(vec![e + d * 14.0, e + n * 6.0, e - n * 6.0], col, Stroke::NONE));
                    } else {
                        p.rect_filled(Rect::from_center_size(e, Vec2::splat(11.0)), 2.0, col);
                    }
                }
            }
        }
        if tool != Tool::Rotate {
            let on = active == Some(Handle::Centre);
            p.circle_filled(c, if on { 7.0 } else { 5.5 }, if on { Color32::WHITE } else { Color32::from_gray(225) });
            p.circle_stroke(c, 7.5, Stroke::new(1.5, Color32::from_black_alpha(110)));
        }
        let _ = g.len_world;
    }

    fn gnomon_items(&self, rect: Rect) -> Vec<(V3, Pos2, bool)> {
        let origin = pos2(rect.right() - 62.0, rect.top() + 62.0);
        let (right, up, _) = self.cam.basis();
        let mut v = Vec::new();
        for i in 0..3 {
            let mut a = [0.0; 3];
            a[i] = 1.0;
            for (sign, pos_end) in [(1.0, true), (-1.0, false)] {
                let d = mul(a, sign);
                let p = origin + vec2(dot(d, right), -dot(d, up)) * 34.0;
                v.push((d, p, pos_end));
            }
        }
        v
    }

    fn gnomon_hit(&self, rect: Rect, p: Pos2) -> Option<V3> {
        self.gnomon_items(rect)
            .into_iter()
            .filter(|(_, c, positive)| *positive && (*c - p).length() < 10.0)
            .map(|(d, _, _)| d)
            .next()
            .or_else(|| self.gnomon_items(rect).into_iter().find(|(_, c, _)| (*c - p).length() < 8.0).map(|(d, _, _)| d))
    }

    fn draw_gnomon(&self, p: &egui::Painter, rect: Rect, mouse: Option<Pos2>) {
        let origin = pos2(rect.right() - 62.0, rect.top() + 62.0);
        p.circle_filled(origin, 48.0, Color32::from_black_alpha(36));
        let mut items = self.gnomon_items(rect);
        let (_, _, fwd) = self.cam.basis();
        // far ones first
        items.sort_by(|a, b| dot(a.0, fwd).total_cmp(&dot(b.0, fwd)).reverse());
        for (d, c, positive) in items {
            let axis = (0..3).find(|&i| d[i].abs() > 0.5).unwrap_or(0);
            let col = style::AXIS[axis];
            let hot = mouse.is_some_and(|m| (m - c).length() < 10.0);
            if positive {
                p.line_segment([origin, c], Stroke::new(2.0, col));
                p.circle_filled(c, if hot { 10.0 } else { 8.5 }, col);
                p.text(c, Align2::CENTER_CENTER, ["X", "Y", "Z"][axis], FontId::proportional(10.5), Color32::from_black_alpha(200));
            } else {
                p.circle_filled(c, if hot { 7.0 } else { 5.5 }, col.gamma_multiply(0.45));
            }
        }
    }

    /// Look straight along a world axis (clicking the gnomon).
    pub fn look_along(&mut self, d: V3) {
        // d points from the centre toward the viewer
        let pitch = d[2].clamp(-1.0, 1.0).asin();
        let yaw = if d[2].abs() > 0.9 { self.cam.yaw } else { d[0].atan2(-d[1]) };
        self.cam.look(yaw, pitch);
    }

    fn pick_at(&mut self, rect: Rect, p: Pos2, keep: bool) {
        let (o, d) = self.cam.ray_at(rect, p);
        let hit = self.field().ray(o, d, 5000.0).and_then(|(_, s)| s.nearest);
        match hit {
            Some((g, s)) => {
                let want = Pick::Shape(g, s);
                self.sel = if keep && self.sel == Some(want) { Some(Pick::Group(g)) } else { Some(want) };
            }
            None => self.sel = None,
        }
    }

    pub fn frame_view(&mut self, only_selected: bool) {
        let t: Vec<(usize, usize)> = if only_selected && self.sel.is_some() {
            targets(&self.scene, self.sel)
        } else {
            self.scene.groups.iter().enumerate().flat_map(|(g, gr)| (0..gr.shapes.len()).map(move |s| (g, s))).collect()
        };
        let shapes: Vec<_> = t.iter().filter_map(|(g, s)| self.scene.shape(*g, *s)).filter(|s| s.visible).collect();
        if shapes.is_empty() {
            self.cam = Camera::default();
            return;
        }
        let mut c = [0.0; 3];
        for s in &shapes {
            c = add(c, s.pos);
        }
        let c = mul(c, 1.0 / shapes.len() as f32);
        let r = shapes.iter().map(|s| len(sub(s.pos, c)) + radius_of(s)).fold(1.0_f32, f32::max);
        self.cam.frame(c, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_about_z_turns_x_into_y() {
        let q = rotate_about([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 90.0);
        assert!(q[0].abs() < 1e-5 && (q[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_group_pick_covers_all_its_shapes() {
        let mut s = Scene::default();
        let extra = s.groups[0].shapes[0].clone();
        s.groups[0].shapes.push(extra);
        assert_eq!(targets(&s, Some(Pick::Group(0))).len(), 2);
        assert_eq!(targets(&s, Some(Pick::Shape(0, 5))).len(), 0);
    }
}
