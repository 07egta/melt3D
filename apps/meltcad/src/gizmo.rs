//! The handles on the selected thing. The default gizmo is one tool that does it all, like Womp3D:
//! arrows to move along an axis, small squares to move in a plane, curved arrows to rotate, the
//! middle dot to resize everything at once, and six small cubes on the faces of the box that stretch
//! the thing along that one direction only (the opposite face stays put). The single tools
//! (move / rotate / scale) are the same handles, one kind at a time.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, vec2};

use crate::app::App;
use crate::camera::Camera;
use crate::model::{Kind, Pick, Scene};
use crate::sdf::{Prepared, V3, add, cross, dot, mul, sub};
use crate::style;
use crate::view::targets;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Just pick things; no handles.
    Select,
    /// Every handle at once (the default).
    All,
    Move,
    Rotate,
    Scale,
}

impl Tool {
    fn moves(self) -> bool {
        matches!(self, Tool::All | Tool::Move)
    }
    fn scales(self) -> bool {
        matches!(self, Tool::All | Tool::Scale)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    /// An arrow (move or, in the scale tool, scale along that axis) or a full ring (rotate tool).
    Axis(usize),
    /// The square for moving in the plane across axis `i`.
    Plane(usize),
    /// A curved arrow that turns about axis `i`.
    Arc(usize),
    /// The cube on a face of the box: `k / 2` is the shape's own axis, odd `k` is the + side.
    Face(usize),
    Centre,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    MoveAxis(usize),
    MovePlane(usize),
    MoveView,
    ScaleAxis(usize),
    ScaleAll,
    StretchFace(usize),
    Rotate(usize),
}

/// A handle being dragged.
#[derive(Clone, Debug)]
pub struct Drag {
    handle: Handle,
    act: Act,
    start: Pos2,
    rect: Rect,
    centre: V3,
    axis: V3,
    screen_dir: Vec2,
    len_px: f32,
    ring_px: f32,
    mm_px: f32,
    /// Where on the axis / in the plane the mouse first was.
    t0: f32,
    p0: V3,
    /// The last angle about the axis seen, and the total turned so far.
    ang_last: f32,
    turned: f32,
    /// Screen directions of the shape's own axes, and the face cube's distance from the middle (px).
    dirs: [Vec2; 3],
    corner_px: f32,
    /// The shape's own axes in the world and its half sizes along them (mm), for stretching a face.
    local: [V3; 3],
    ext: V3,
    /// (group, shape, position, rotation, scale) when the drag began.
    from: Vec<(usize, usize, V3, V3, V3)>,
    /// A linked copy being moved: its group and where it stood when the drag began.
    link: Option<(usize, crate::pattern::Xform)>,
}

impl Drag {
    pub fn handle(&self) -> Handle {
        self.handle
    }
}

const RING_SEGS: usize = 56;
const ARROW_PX: f32 = 78.0;
// Hit distances are egui logical points, so physical click targets grow on high-DPI screens.
const HANDLE_HIT_PX: f32 = 11.0;

pub struct Geom {
    centre: Pos2,
    centre_w: V3,
    axes: [V3; 3],
    ends: [Option<Pos2>; 3],
    planes: [Option<[Pos2; 4]>; 3],
    /// Full rings (rotate tool) or short curved arrows (the all-in-one gizmo).
    arcs: [Vec<Pos2>; 3],
    corners: Option<[Option<Pos2>; 8]>,
    /// The cubes on the six faces of the box (the stretch handles).
    faces: Option<[Option<Pos2>; 6]>,
    local: [V3; 3],
    ext: V3,
    len_w: f32,
}

const WORLD: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Half the size of a shape's box in its own axes, in mm (scale included).
pub fn half_extents(s: &crate::model::Shape) -> V3 {
    let h = match s.kind {
        Kind::Sphere => [s.size[0]; 3],
        Kind::Box => [s.size[0] * 0.5, s.size[1] * 0.5, s.size[2] * 0.5],
        Kind::Cylinder | Kind::Cone => [s.size[0], s.size[0], s.size[1] * 0.5],
        Kind::Torus => [s.size[0] + s.size[1], s.size[0] + s.size[1], s.size[1]],
        Kind::Extrude => {
            let x = s.outline.iter().map(|p| p[0].abs()).fold(1.0_f32, f32::max);
            let y = s.outline.iter().map(|p| p[1].abs()).fold(1.0_f32, f32::max);
            [x, y, s.size[0] * 0.5]
        }
        _ => s.local_half(),
    };
    [h[0] * s.scale[0], h[1] * s.scale[1], h[2] * s.scale[2]]
}

fn plane_axes(i: usize) -> (usize, usize) {
    ((i + 1) % 3, (i + 2) % 3)
}

/// `span` is (from angle, to angle, segments).
fn arc_points(cam: &Camera, rect: Rect, c: V3, axis: usize, r: f32, span: (f32, f32, usize)) -> Vec<Pos2> {
    let (from, to, n) = span;
    let (a, b) = plane_axes(axis);
    (0..=n)
        .filter_map(|i| {
            let t = from + (to - from) * i as f32 / n as f32;
            cam.project(rect, add(c, add(mul(WORLD[a], r * t.cos()), mul(WORLD[b], r * t.sin()))))
        })
        .collect()
}

/// The handles for the current selection, placed on screen.
pub fn build(cam: &Camera, rect: Rect, scene: &Scene, sel: Option<Pick>, tool: Tool) -> Option<Geom> {
    build_multi(cam, rect, scene, sel.as_slice(), tool)
}

/// The handles for one pick or several: with several they sit in the middle of all and act on all.
pub fn build_multi(cam: &Camera, rect: Rect, scene: &Scene, picks: &[Pick], tool: Tool) -> Option<Geom> {
    if tool == Tool::Select {
        return None;
    }
    let sel = if let [one] = picks { Some(*one) } else { None };
    let centre_w = crate::view::centre_of_picks(scene, picks)?;
    let c = cam.project(rect, centre_w)?;
    let len_w = ARROW_PX * cam.mm_per_px(centre_w, rect.height());
    let shape = match sel {
        Some(Pick::Shape(g, s)) => scene.shape(g, s),
        _ => None,
    };
    let local = shape.map(|s| Prepared::new(s).inv).unwrap_or(WORLD);
    let ext = shape.map(half_extents).unwrap_or([0.0; 3]);
    // arrows follow the world; in the scale tool they follow the shape
    let axes = if tool == Tool::Scale { local } else { WORLD };
    let ends = [0, 1, 2].map(|i| cam.project(rect, add(centre_w, mul(axes[i], len_w))));
    let planes = [0, 1, 2].map(|i| {
        if !tool.moves() {
            return None;
        }
        let (a, b) = plane_axes(i);
        let corner = |u: f32, v: f32| cam.project(rect, add(centre_w, add(mul(axes[a], len_w * u), mul(axes[b], len_w * v))));
        Some([corner(0.28, 0.28)?, corner(0.56, 0.28)?, corner(0.56, 0.56)?, corner(0.28, 0.56)?])
    });
    let arcs = [0, 1, 2].map(|i| {
        if tool == Tool::Rotate {
            arc_points(cam, rect, centre_w, i, len_w * 0.9, (0.0, std::f32::consts::TAU, RING_SEGS))
        } else if tool == Tool::All {
            arc_points(cam, rect, centre_w, i, len_w * 1.16, (0.22, 1.36, 20))
        } else {
            Vec::new()
        }
    });
    let corners = if tool.scales() && shape.is_some() {
        Some(std::array::from_fn(|k| {
            let s = |bit: usize| if (k >> bit) & 1 == 1 { 1.0 } else { -1.0 };
            let off = add(add(mul(local[0], s(0) * ext[0]), mul(local[1], s(1) * ext[1])), mul(local[2], s(2) * ext[2]));
            cam.project(rect, add(centre_w, off))
        }))
    } else {
        None
    };
    let faces = if tool.scales() && shape.is_some() {
        Some(std::array::from_fn(|k| {
            let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
            let i = k / 2;
            cam.project(rect, add(centre_w, mul(local[i], sign * ext[i])))
        }))
    } else {
        None
    };
    Some(Geom { centre: c, centre_w, axes, ends, planes, arcs, corners, faces, local, ext, len_w })
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq().max(1e-6);
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn in_quad(p: Pos2, q: &[Pos2; 4]) -> bool {
    let mut sign = 0.0_f32;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c.abs() > 1e-6 {
            if sign != 0.0 && c.signum() != sign {
                return false;
            }
            sign = c.signum();
        }
    }
    true
}

fn poly_dist(p: Pos2, pts: &[Pos2]) -> f32 {
    pts.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f32::MAX, f32::min)
}

/// Which handle is under `p`. Nearer, smaller things win over big ones.
pub fn hit(g: &Geom, tool: Tool, p: Pos2) -> Option<Handle> {
    let near = |d: f32, r: f32| d < r;
    if tool != Tool::Rotate && near((p - g.centre).length(), HANDLE_HIT_PX) {
        return Some(Handle::Centre);
    }
    if let Some(faces) = &g.faces {
        for (k, c) in faces.iter().enumerate() {
            if let Some(c) = c
                && near((p - *c).length(), HANDLE_HIT_PX)
            {
                return Some(Handle::Face(k));
            }
        }
    }
    for (i, q) in g.planes.iter().enumerate() {
        if let Some(q) = q
            && in_quad(p, q)
        {
            return Some(Handle::Plane(i));
        }
    }
    let mut best: Option<(f32, Handle)> = None;
    let mut consider = |d: f32, h: Handle, r: f32| {
        if d < r && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, h));
        }
    };
    for i in 0..3 {
        if tool == Tool::Rotate {
            consider(poly_dist(p, &g.arcs[i]), Handle::Axis(i), HANDLE_HIT_PX);
        } else {
            if tool != Tool::Rotate
                && let Some(e) = g.ends[i]
            {
                consider(seg_dist(p, g.centre, e), Handle::Axis(i), HANDLE_HIT_PX);
            }
            if tool == Tool::All {
                consider(poly_dist(p, &g.arcs[i]), Handle::Arc(i), HANDLE_HIT_PX);
            }
        }
    }
    best.map(|(_, h)| h)
}

pub fn draw(p: &Painter, g: &Geom, tool: Tool, active: Option<Handle>) {
    let c = g.centre;
    let col_of = |i: usize, on: bool| if on { Color32::WHITE } else { style::AXIS[i] };
    // the box
    if let Some(corners) = &g.corners {
        let edge = |a: usize, b: usize| {
            if let (Some(pa), Some(pb)) = (corners[a], corners[b]) {
                p.line_segment([pa, pb], Stroke::new(1.0, Color32::from_white_alpha(70)));
            }
        };
        for a in 0..8usize {
            for bit in 0..3 {
                let b = a ^ (1 << bit);
                if a < b {
                    edge(a, b);
                }
            }
        }
    }
    // squares for moving in a plane
    for (i, q) in g.planes.iter().enumerate() {
        let Some(q) = q else { continue };
        let on = active == Some(Handle::Plane(i));
        let col = style::AXIS[i];
        p.add(Shape::convex_polygon(q.to_vec(), col.gamma_multiply(if on { 0.75 } else { 0.38 }), Stroke::new(1.5, col_of(i, on))));
    }
    // curved arrows (or full rings) for turning
    for i in 0..3 {
        let on = match active {
            Some(Handle::Arc(a)) => a == i,
            Some(Handle::Axis(a)) => a == i && tool == Tool::Rotate,
            _ => false,
        };
        let pts = &g.arcs[i];
        if pts.len() < 2 {
            continue;
        }
        let col = col_of(i, on);
        p.add(Shape::line(pts.clone(), Stroke::new(if on { 5.0 } else { 3.5 }, col)));
        if tool == Tool::All {
            let (Some(last), Some(prev)) = (pts.last(), pts.get(pts.len() - 2)) else { continue };
            let d = (*last - *prev).normalized();
            let n = vec2(-d.y, d.x);
            p.add(Shape::convex_polygon(vec![*last + d * 11.0, *last + n * 6.5, *last - n * 6.5], col, Stroke::NONE));
            if let (Some(first), Some(next)) = (pts.first(), pts.get(1)) {
                let d = (*first - *next).normalized();
                let n = vec2(-d.y, d.x);
                p.add(Shape::convex_polygon(vec![*first + d * 11.0, *first + n * 6.5, *first - n * 6.5], col, Stroke::NONE));
            }
        }
    }
    // arrows (or scale cubes)
    if tool != Tool::Rotate {
        for i in 0..3 {
            let Some(e) = g.ends[i] else { continue };
            let on = active == Some(Handle::Axis(i));
            let col = col_of(i, on);
            p.line_segment([c, e], Stroke::new(if on { 3.5 } else { 2.5 }, col));
            let d = (e - c).normalized();
            if tool == Tool::Scale {
                p.rect_filled(Rect::from_center_size(e, Vec2::splat(11.0)), 2.0, col);
            } else {
                let n = vec2(-d.y, d.x);
                p.add(Shape::convex_polygon(vec![e + d * 15.0, e + n * 6.5, e - n * 6.5], col, Stroke::NONE));
            }
        }
    }
    // the cubes on the faces: each stretches the thing along its own direction only
    if let Some(faces) = &g.faces {
        for (k, pt) in faces.iter().enumerate() {
            let Some(pt) = pt else { continue };
            let on = active == Some(Handle::Face(k));
            let r = if on { 6.5 } else { 5.0 };
            let col = if on { Color32::WHITE } else { style::AXIS[k / 2] };
            let rect = Rect::from_center_size(*pt, Vec2::splat(r * 2.0));
            p.rect_filled(rect, 2.0, col);
            p.rect_stroke(rect, 2.0, Stroke::new(1.2, Color32::from_black_alpha(150)), egui::StrokeKind::Outside);
        }
    }
    if tool != Tool::Rotate {
        let on = active == Some(Handle::Centre);
        p.circle_filled(c, if on { 7.5 } else { 6.0 }, if on { Color32::WHITE } else { Color32::from_gray(232) });
        p.circle_stroke(c, if on { 8.0 } else { 6.8 }, Stroke::new(1.5, Color32::from_black_alpha(120)));
    }
    let _ = (g.len_w, g.centre_w, g.ext, g.local, g.axes);
}

/// Where a ray meets the plane through `c` with normal `n`.
fn ray_plane(o: V3, d: V3, c: V3, n: V3) -> Option<V3> {
    let den = dot(d, n);
    if den.abs() < 0.04 {
        return None;
    }
    let t = dot(sub(c, o), n) / den;
    (t.is_finite() && t > 0.0).then(|| add(o, mul(d, t)))
}

/// The distance along the line `c + t a` of its closest approach to the ray `o + s d`.
fn closest_on_axis(o: V3, d: V3, c: V3, a: V3) -> Option<f32> {
    let w0 = sub(o, c);
    let b = dot(d, a);
    let den = 1.0 - b * b;
    if den < 1e-3 {
        return None;
    }
    let t = (dot(a, w0) - b * dot(d, w0)) / den;
    t.is_finite().then_some(t)
}

fn rotate_about(p: V3, axis: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    add(add(mul(p, c), mul(cross(axis, p), s)), mul(axis, dot(axis, p) * (1.0 - c)))
}

fn angle_in_plane(v: V3, axis: usize) -> f32 {
    let (a, b) = plane_axes(axis);
    dot(v, WORLD[b]).atan2(dot(v, WORLD[a]))
}

impl App {
    fn action_for(&self, handle: Handle, tool: Tool, shift: bool) -> Act {
        let group = matches!(self.sel, Some(Pick::Group(_))) || self.picks().len() > 1;
        match (tool, handle) {
            (_, Handle::Plane(i)) => Act::MovePlane(i),
            (_, Handle::Arc(i)) | (Tool::Rotate, Handle::Axis(i)) => Act::Rotate(i),
            (_, Handle::Face(k)) => Act::StretchFace(k),
            (Tool::Scale, Handle::Axis(i)) if !group => Act::ScaleAxis(i),
            (Tool::Scale, _) => Act::ScaleAll,
            (Tool::All, Handle::Centre) if !shift => Act::ScaleAll,
            (_, Handle::Centre) => Act::MoveView,
            (_, Handle::Axis(i)) => Act::MoveAxis(i),
        }
    }

    pub fn begin_drag(&self, handle: Handle, g: &Geom, rect: Rect, mouse: Pos2, tool: Tool, shift: bool) -> Drag {
        let act = self.action_for(handle, tool, shift);
        let axis_i = match handle {
            Handle::Axis(i) | Handle::Arc(i) | Handle::Plane(i) => i,
            _ => 0,
        };
        let axis = match act {
            Act::Rotate(i) => WORLD[i],
            Act::MovePlane(i) => WORLD[i],
            _ => g.axes[axis_i],
        };
        let end = g.ends[axis_i].unwrap_or(g.centre + vec2(1.0, 0.0));
        let dir = end - g.centre;
        let len_px = dir.length().max(8.0);
        let screen_dir = if dir.length() > 1e-3 { dir / dir.length() } else { vec2(1.0, 0.0) };
        let (o, rd) = self.cam.ray_at(rect, mouse);
        let (mut t0, mut p0) = (0.0, g.centre_w);
        match act {
            Act::MoveAxis(_) | Act::ScaleAxis(_) => t0 = closest_on_axis(o, rd, g.centre_w, axis).unwrap_or(0.0),
            Act::MovePlane(i) => p0 = ray_plane(o, rd, g.centre_w, WORLD[i]).unwrap_or(g.centre_w),
            _ => {}
        }
        let (mut ang0, mut ring_px, mut tangent) = (0.0, 60.0, screen_dir);
        if let Act::Rotate(i) = act {
            if let Some(p) = ray_plane(o, rd, g.centre_w, WORLD[i]) {
                ang0 = angle_in_plane(sub(p, g.centre_w), i);
            }
            let pts = &g.arcs[axis_i];
            if !pts.is_empty() {
                ring_px = pts.iter().map(|p| (*p - g.centre).length()).sum::<f32>() / pts.len() as f32;
                if let Some((k, _)) = pts.iter().enumerate().min_by(|a, b| (*a.1 - mouse).length().total_cmp(&(*b.1 - mouse).length())) {
                    let a = pts[k.saturating_sub(1)];
                    let b = pts[(k + 1).min(pts.len() - 1)];
                    let t = b - a;
                    if t.length() > 1e-3 {
                        tangent = t / t.length();
                    }
                }
            }
        }
        let mut dirs = [vec2(1.0, 0.0); 3];
        for (i, d) in dirs.iter_mut().enumerate() {
            if let Some(e) = self.cam.project(rect, add(g.centre_w, mul(g.local[i], g.len_w))) {
                *d = (e - g.centre) / ARROW_PX;
            }
        }
        let corner_px = match (act, g.faces) {
            (Act::StretchFace(k), Some(fs)) => fs.get(k).copied().flatten().map(|p| (p - g.centre).length()).unwrap_or(60.0),
            _ => 60.0,
        };
        let from = crate::view::targets_all(&self.scene, &self.picks())
            .into_iter()
            .filter_map(|(gi, si)| self.scene.shape(gi, si).map(|s| (gi, si, s.pos, s.rot, s.scale)))
            .collect();
        let link = match self.sel.filter(|_| self.picks().len() == 1) {
            Some(Pick::Group(gi)) => self.scene.groups.get(gi).filter(|gr| gr.is_linked()).map(|gr| (gi, gr.xform)),
            _ => None,
        };
        Drag {
            handle,
            act,
            start: mouse,
            rect,
            centre: g.centre_w,
            axis,
            screen_dir: if matches!(act, Act::Rotate(_)) { tangent } else { screen_dir },
            len_px,
            ring_px: ring_px.max(20.0),
            mm_px: self.cam.mm_per_px(g.centre_w, rect.height().max(1.0)),
            t0,
            p0,
            ang_last: ang0,
            turned: 0.0,
            dirs,
            corner_px: corner_px.max(14.0),
            local: g.local,
            ext: g.ext,
            from,
            link,
        }
    }

    /// Cancel a gizmo gesture without generating an undo step.
    pub(crate) fn cancel_gizmo_drag(&mut self) -> bool {
        let Some(d) = self.drag.take() else { return false };
        if let Some((gi, xform)) = d.link
            && let Some(group) = self.scene.groups.get_mut(gi)
        {
            group.xform = xform;
        }
        for (gi, si, pos, rot, scale) in d.from {
            if let Some(shape) = self.scene.shape_mut(gi, si) {
                shape.pos = pos;
                shape.rot = rot;
                shape.scale = scale;
            }
        }
        self.orbiting = false;
        true
    }

    pub fn apply_drag(&mut self, d: &mut Drag, mouse: Pos2, snap: bool) {
        let delta = mouse - d.start;
        let group = matches!(self.sel, Some(Pick::Group(_))) || self.picks().len() > 1;
        let (right, up, _) = self.cam.basis();
        let (o, rd) = self.cam.ray_at(d.rect, mouse);
        let round = |v: f32, step: f32| if snap { (v / step).round() * step } else { v };
        // what the mouse has turned the thing by, this frame
        let mut turn_deg = 0.0;
        if let Act::Rotate(i) = d.act {
            let from_plane = ray_plane(o, rd, d.centre, WORLD[i]).map(|p| angle_in_plane(sub(p, d.centre), i));
            turn_deg = match from_plane {
                Some(a) => {
                    let mut step = a - d.ang_last;
                    while step > std::f32::consts::PI {
                        step -= std::f32::consts::TAU;
                    }
                    while step < -std::f32::consts::PI {
                        step += std::f32::consts::TAU;
                    }
                    d.ang_last = a;
                    d.turned += step;
                    d.turned.to_degrees()
                }
                None => delta.dot(d.screen_dir) / d.ring_px * 57.2958,
            };
            turn_deg = round(turn_deg, 15.0);
        }
        // a linked copy moves, turns or grows as a whole (about the middle of its source)
        if let Some((gi, x0)) = d.link
            && let Some(gr) = self.scene.groups.get_mut(gi)
        {
            let mut x = x0;
            match d.act {
                Act::MoveAxis(_) => {
                    let amt = match closest_on_axis(o, rd, d.centre, d.axis) {
                        Some(t) => t - d.t0,
                        None => delta.dot(d.screen_dir) * d.mm_px * (ARROW_PX / d.len_px),
                    };
                    x.pos = add(x0.pos, mul(d.axis, round(amt, 1.0)));
                }
                Act::MovePlane(i) => {
                    if let Some(p) = ray_plane(o, rd, d.centre, WORLD[i]) {
                        x.pos = add(x0.pos, sub(p, d.p0));
                    }
                }
                Act::MoveView => x.pos = add(x0.pos, add(mul(right, delta.x * d.mm_px), mul(up, -delta.y * d.mm_px))),
                Act::Rotate(i) => x.rot = crate::pattern::turn_euler(x0.rot, i as u8, turn_deg),
                Act::ScaleAxis(_) | Act::ScaleAll | Act::StretchFace(_) => {
                    let f = ((delta.x - delta.y) / 160.0).exp();
                    x.scale = round(x0.scale * f, 0.05).clamp(0.01, 100.0);
                }
            }
            if snap && matches!(d.act, Act::MoveAxis(_) | Act::MovePlane(_) | Act::MoveView) {
                x.pos = [round(x.pos[0], 1.0), round(x.pos[1], 1.0), round(x.pos[2], 1.0)];
            }
            gr.xform = x;
        }
        for &(gi, si, pos, rot, scale) in &d.from {
            let Some(s) = self.scene.shape_mut(gi, si) else { continue };
            match d.act {
                Act::MoveAxis(_) => {
                    let amt = match closest_on_axis(o, rd, d.centre, d.axis) {
                        Some(t) => t - d.t0,
                        None => delta.dot(d.screen_dir) * d.mm_px * (ARROW_PX / d.len_px),
                    };
                    s.pos = add(pos, mul(d.axis, round(amt, 1.0)));
                    if snap {
                        s.pos = [round(s.pos[0], 1.0), round(s.pos[1], 1.0), round(s.pos[2], 1.0)];
                    }
                }
                Act::MovePlane(i) => {
                    if let Some(p) = ray_plane(o, rd, d.centre, WORLD[i]) {
                        s.pos = add(pos, sub(p, d.p0));
                        if snap {
                            s.pos = [round(s.pos[0], 1.0), round(s.pos[1], 1.0), round(s.pos[2], 1.0)];
                        }
                    }
                }
                Act::MoveView => {
                    s.pos = add(pos, add(mul(right, delta.x * d.mm_px), mul(up, -delta.y * d.mm_px)));
                    if snap {
                        s.pos = [round(s.pos[0], 1.0), round(s.pos[1], 1.0), round(s.pos[2], 1.0)];
                    }
                }
                Act::Rotate(i) => {
                    s.rot[i] = (rot[i] + turn_deg).rem_euclid(360.0);
                    if group {
                        s.pos = add(d.centre, rotate_about(sub(pos, d.centre), WORLD[i], turn_deg));
                    }
                }
                Act::ScaleAxis(i) => {
                    let f = (1.0 + delta.dot(d.screen_dir) / d.len_px).max(0.05);
                    s.scale[i] = round(scale[i] * f, 0.05).clamp(0.01, 100.0);
                }
                Act::ScaleAll => {
                    let f = ((delta.x - delta.y) / 160.0).exp();
                    for (dst, src) in s.scale.iter_mut().zip(scale) {
                        *dst = round(src * f, 0.05).clamp(0.01, 100.0);
                    }
                    if group {
                        s.pos = add(d.centre, mul(sub(pos, d.centre), f));
                    }
                }
                Act::StretchFace(k) => {
                    // one direction only; the opposite face stays where it is
                    let i = k / 2;
                    let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
                    let f = (1.0 + sign * delta.dot(d.dirs[i]) / d.corner_px).max(0.05);
                    let new = round(scale[i] * f, 0.05).clamp(0.01, 100.0);
                    let real = new / scale[i].max(1e-6);
                    s.scale[i] = new;
                    let shift = sign * d.ext[i] * (real - 1.0);
                    s.pos = add(pos, mul(d.local[i], shift));
                }
            }
        }
    }
}

/// The handle's direction on the floor for the status line.
pub fn hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Click to select",
        Tool::All => "Arrow: move   Square: move on plane   Curved arrow: rotate   Center: scale all   Face square: stretch that way",
        Tool::Move => "Drag arrow to move (Ctrl: 1 mm steps)",
        Tool::Rotate => "Drag ring to rotate (Ctrl: 15 degree steps)",
        Tool::Scale => "Center: scale all   Face square: stretch that way   Axis square: scale that axis",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn rotating_about_z_turns_x_into_y() {
        let q = rotate_about([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 90.0);
        assert!(q[0].abs() < 1e-5 && (q[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_closest_point_on_an_axis_is_found() {
        // a ray straight down at x = 5 meets the x axis at 5
        let t = closest_on_axis([5.0, 0.0, 50.0], [0.0, 0.0, -1.0], [0.0; 3], [1.0, 0.0, 0.0]);
        assert!(t.is_some_and(|t| (t - 5.0).abs() < 1e-4));
        // a ray along the axis has no answer
        assert!(closest_on_axis([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn a_ray_meets_a_plane_only_in_front() {
        let p = ray_plane([0.0, 0.0, 10.0], [0.0, 0.0, -1.0], [0.0; 3], [0.0, 0.0, 1.0]);
        assert!(p.is_some_and(|p| p[2].abs() < 1e-5));
        assert!(ray_plane([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], [0.0; 3], [0.0, 0.0, 1.0]).is_none());
    }

    #[test]
    fn small_handles_have_a_logical_point_hit_margin() {
        let app = App::new(true, None);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let Some(g) = build(&app.cam, rect, &app.scene, Some(Pick::Shape(0, 0)), Tool::All) else { panic!("gizmo") };
        assert_eq!(hit(&g, Tool::All, g.centre + vec2(9.5, 0.0)), Some(Handle::Centre));
        assert_eq!(hit(&g, Tool::All, g.centre), Some(Handle::Centre));
    }

    #[test]
    fn a_point_inside_a_square_is_found() {
        let q = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];
        assert!(in_quad(pos2(5.0, 5.0), &q) && !in_quad(pos2(15.0, 5.0), &q));
    }

    #[test]
    fn stretching_a_face_changes_one_axis_and_keeps_the_opposite_face_in_place() {
        let mut app = App::new(true, None);
        if let Some(s) = app.scene.shape_mut(0, 0) {
            s.kind = Kind::Box;
            s.size = [30.0, 30.0, 30.0];
            s.pos = [0.0, 0.0, 15.0];
            s.rot = [0.0; 3];
            s.scale = [1.0; 3];
        }
        app.sel = Some(Pick::Shape(0, 0));
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let Some(g) = build(&app.cam, rect, &app.scene, app.sel, Tool::All) else { panic!("gizmo") };
        let Some(face) = g.faces.and_then(|f| f[1]) else { panic!("the + x face cube is on screen") };
        assert_eq!(hit(&g, Tool::All, face), Some(Handle::Face(1)));
        let mut d = app.begin_drag(Handle::Face(1), &g, rect, face, Tool::All, false);
        let out = d.dirs[0].normalized() * d.corner_px * 0.5;
        app.apply_drag(&mut d, face + out, false);
        let Some(s) = app.scene.shape(0, 0) else { panic!("shape") };
        assert!(s.scale[0] > 1.1 && s.scale[1] == 1.0 && s.scale[2] == 1.0, "{:?}", s.scale);
        let min_x = s.pos[0] - 15.0 * s.scale[0];
        assert!((min_x + 15.0).abs() < 0.2, "the - x face stayed at -15, now {min_x}");
        assert!(s.pos[1] == 0.0 && s.pos[2] == 15.0);
    }

    #[test]
    fn the_centre_scales_everything_and_the_face_cubes_are_six() {
        let app = App::new(true, None);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let Some(g) = build(&app.cam, rect, &app.scene, Some(Pick::Shape(0, 0)), Tool::All) else { panic!("gizmo") };
        assert_eq!(g.faces.map(|f| f.len()), Some(6));
        assert_eq!(hit(&g, Tool::All, g.centre), Some(Handle::Centre));
        assert_eq!(app.action_for(Handle::Centre, Tool::All, false), Act::ScaleAll);
    }

    #[test]
    fn a_linked_copy_is_moved_turned_and_scaled_by_its_handles() {
        let mut app = App::new(true, None);
        app.make_instance(&egui::Context::default());
        let Some(Pick::Group(c)) = app.sel else { panic!("the copy is selected") };
        let x0 = app.scene.groups[c].xform;
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let Some(g) = build(&app.cam, rect, &app.scene, app.sel, Tool::All) else { panic!("a linked copy has handles") };
        // the handles sit on the middle of the source, moved with the copy
        assert!((g.centre_w[0] - (0.0 + x0.pos[0])).abs() < 1e-3 && (g.centre_w[2] - 20.0).abs() < 1e-3, "{:?}", g.centre_w);
        // an arrow moves it
        let tip = g.ends[0].unwrap_or(g.centre);
        let mut d = app.begin_drag(Handle::Axis(0), &g, rect, g.centre + (tip - g.centre) * 0.5, Tool::All, false);
        app.apply_drag(&mut d, g.centre + (tip - g.centre) * 0.5 + (tip - g.centre).normalized() * 30.0, false);
        let moved = app.scene.groups[c].xform;
        assert!(moved.pos[0] != x0.pos[0] && moved.pos[1] == x0.pos[1] && moved.rot == x0.rot);
        // the centre grows it
        let mut d = app.begin_drag(Handle::Centre, &g, rect, g.centre, Tool::All, false);
        app.apply_drag(&mut d, g.centre + vec2(40.0, -40.0), false);
        assert!(app.scene.groups[c].xform.scale > 1.2, "{}", app.scene.groups[c].xform.scale);
        // a ring turns it about the Z axis
        app.scene.groups[c].xform.scale = 1.0;
        let Some(g) = build(&app.cam, rect, &app.scene, app.sel, Tool::Rotate) else { panic!("gizmo") };
        let on_ring = g.arcs[2].get(g.arcs[2].len() / 4).copied().unwrap_or(g.centre);
        let mut d = app.begin_drag(Handle::Axis(2), &g, rect, on_ring, Tool::Rotate, false);
        let next = g.arcs[2].get(g.arcs[2].len() / 4 + 6).copied().unwrap_or(g.centre);
        app.apply_drag(&mut d, next, false);
        assert!(app.scene.groups[c].xform.rot[2].abs() > 1.0, "{:?}", app.scene.groups[c].xform.rot);
        // the source group did not move at all
        assert_eq!(app.scene.groups[0].shapes[0].pos, [0.0, 0.0, 20.0]);
    }

    #[test]
    fn a_sphere_box_is_its_radius() {
        let s = crate::model::Shape::new(Kind::Sphere, [0.0; 3], Default::default());
        assert_eq!(half_extents(&s), [20.0; 3]);
    }
}
