//! Live patterns, mirrors and linked copies of a group.
//!
//! A group may carry a `Pattern`: mirror, rectangular, circular, along a path, or on the points of
//! other geometry. It makes many *instances* of the whole group without using a shape slot. An
//! instance is a transform: a turn or flip (a 3x3 matrix with determinant +1 or -1), a move, and
//! a uniform scale. The shader and the CPU `Field` measure the group at the point carried back into
//! the group's own space and melt the instances one after the other into the scene, exactly as if
//! each were a separate group with the group's own op and blend.
//!
//! A group may also be a *linked copy* of another group (`Group::instance_of`, the other group's
//! `uid`): it has no shapes of its own, it shows the source's shapes through its own move / turn /
//! scale (`Group::xform`, about the source's centre), and whatever is edited in the source shows in
//! every copy.
//!
//! Everything here is plain numbers, so it is tested without a GPU. All inputs are hostile: files
//! hold the numbers, so counts and values are clamped and the total is capped at `MAX_INSTANCES`.

use serde::{Deserialize, Serialize};

use crate::accel::Bound;
use crate::model::{Group, Kind, Scene, Shape};
use crate::sdf::{V3, add, cross, dot, len, mul, norm, rotation, sub};

/// Most instances in a scene (counting the linked copies too): the shader loops over them.
pub const MAX_INSTANCES: usize = 256;
/// Most copies along one side of a rectangular pattern (the whole scene is held to `MAX_INSTANCES`).
pub const MAX_SIDE: u32 = 64;
const COORD: f32 = 5000.0;

/// What `Scene` says when the instances do not all fit.
pub const TOO_MANY: &str = "パターンのこすうが多すぎます（シーン全体で256こまで）";

// ---------------------------------------------------------------------------------------------
// The transform of one instance

/// One instance: `world = s * M * q + t` for a point `q` of the group's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inst {
    /// The rows of M (turns and flips, never stretches).
    pub m: [V3; 3],
    pub t: V3,
    pub s: f32,
}

type Mat = [V3; 3];

const EYE: Mat = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [[0.0_f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}

fn mat_vec(a: &Mat, v: V3) -> V3 {
    [dot(a[0], v), dot(a[1], v), dot(a[2], v)]
}

fn transpose(a: &Mat) -> Mat {
    [[a[0][0], a[1][0], a[2][0]], [a[0][1], a[1][1], a[2][1]], [a[0][2], a[1][2], a[2][2]]]
}

fn det(a: &Mat) -> f32 {
    dot(a[0], cross(a[1], a[2]))
}

impl Inst {
    pub const IDENTITY: Inst = Inst { m: EYE, t: [0.0; 3], s: 1.0 };

    /// Only a move.
    pub fn moved(t: V3) -> Inst {
        Inst { t, ..Inst::IDENTITY }
    }

    /// Turn by `m` about the point `c` (the point stays where it is).
    pub fn about(m: Mat, c: V3) -> Inst {
        Inst { m, t: sub(c, mat_vec(&m, c)), s: 1.0 }
    }

    /// Where a point of the group goes.
    pub fn apply(&self, q: V3) -> V3 {
        add(mul(mat_vec(&self.m, q), self.s), self.t)
    }

    /// The point of the group that lands on the world point `w`.
    pub fn back(&self, w: V3) -> V3 {
        mul(mat_vec(&transpose(&self.m), sub(w, self.t)), 1.0 / self.s.max(1e-6))
    }

    /// The three rows `[x, y, z, offset]` of the map from the world back into the group, so that
    /// `q = rows . (w, 1)`. This is what the shader reads.
    pub fn back_rows(&self) -> [[f32; 4]; 3] {
        let inv_s = 1.0 / self.s.max(1e-6);
        let mt = transpose(&self.m);
        let mut o = [[0.0_f32; 4]; 3];
        for i in 0..3 {
            let row = mul(mt[i], inv_s);
            o[i] = [row[0], row[1], row[2], -dot(row, self.t)];
        }
        o
    }

    /// First this, then `outer`.
    pub fn then(&self, outer: &Inst) -> Inst {
        Inst { m: mat_mul(&outer.m, &self.m), t: add(mul(mat_vec(&outer.m, self.t), outer.s), outer.t), s: self.s * outer.s }
    }

    /// Does it mirror (turn a right hand into a left one)?
    pub fn flips(&self) -> bool {
        det(&self.m) < 0.0
    }

    /// A sphere carried along.
    pub fn bound(&self, b: Bound) -> Bound {
        Bound { c: self.apply(b.c), r: b.r * self.s }
    }

    fn finite(&self) -> bool {
        self.m.iter().flatten().chain(self.t.iter()).all(|v| v.is_finite()) && self.s.is_finite() && self.s > 0.0
    }
}

/// The turn about a world axis (0 = X, 1 = Y, 2 = Z) by `deg` degrees.
fn axis_turn(axis: u8, deg: f32) -> Mat {
    let mut r = [0.0_f32; 3];
    r[usize::from(axis.min(2))] = deg;
    rotation(r)
}

/// A turn that takes the X axis to the direction `t` (the Z axis stays upright when it can).
fn frame_along(t: V3) -> Mat {
    let x = norm(t);
    let up = if x[2].abs() > 0.999 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let y = norm(cross(up, x));
    let z = cross(x, y);
    // columns x, y, z
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

// ---------------------------------------------------------------------------------------------
// The patterns

/// Where the points of a geometry pattern come from.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    /// The nodes, outline points or corners of a shape (its `uid`).
    Shape(u32),
    /// The middle of every shape of a group (its `uid`).
    Group(u32),
}

/// How a path pattern spaces its copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathMode {
    /// `count` copies spread evenly from one end of the path to the other.
    Even,
    /// A copy every `step` mm along the path (at most `count`).
    Step,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// Flip across the world planes through the origin (or the group's middle): X, Y, Z.
    Mirror { axes: [bool; 3], centre: bool },
    /// A grid of `counts` copies `spacing` mm apart (the first stays where it is).
    Rect { counts: [u32; 3], spacing: [f32; 3] },
    /// `count` copies turned about the axis (0 X, 1 Y, 2 Z) through `centre`, over `angle` degrees.
    /// `rotate`: the copies turn with the pattern; otherwise they keep their direction.
    Circular { count: u32, angle: f32, axis: u8, centre: [f32; 3], rotate: bool },
    /// Copies of the group's middle on the path of a curve, sweep or outline shape (its `uid`).
    Path { source: u32, count: u32, mode: PathMode, step: f32, align: bool },
    /// One copy on every point of other geometry; `orient` turns each along the geometry.
    Geometry { source: Source, orient: bool },
}

/// The five kinds, for the selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Mirror,
    Rect,
    Circular,
    Path,
    Geometry,
}

impl PatternKind {
    pub const ALL: [PatternKind; 5] = [PatternKind::Mirror, PatternKind::Rect, PatternKind::Circular, PatternKind::Path, PatternKind::Geometry];
    pub fn label(self) -> &'static str {
        match self {
            PatternKind::Mirror => "ミラー",
            PatternKind::Rect => "矩形状",
            PatternKind::Circular => "円形状",
            PatternKind::Path => "パス上",
            PatternKind::Geometry => "ジオメトリ",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            PatternKind::Mirror => "X・Y・Zの面で左右（上下・前後）に写します",
            PatternKind::Rect => "たて・よこ・高さに、同じ間かくでならべます",
            PatternKind::Circular => "ある軸のまわりに、ぐるっとならべます",
            PatternKind::Path => "線（カーブ・スイープ・かたち）にそってならべます",
            PatternKind::Geometry => "ほかの形の点（ふし・かど）ごとに 1こずつおきます",
        }
    }
}

impl Pattern {
    pub fn kind(&self) -> PatternKind {
        match self {
            Pattern::Mirror { .. } => PatternKind::Mirror,
            Pattern::Rect { .. } => PatternKind::Rect,
            Pattern::Circular { .. } => PatternKind::Circular,
            Pattern::Path { .. } => PatternKind::Path,
            Pattern::Geometry { .. } => PatternKind::Geometry,
        }
    }

    /// A first pattern of that kind, with sensible numbers.
    pub fn starting(kind: PatternKind) -> Pattern {
        match kind {
            PatternKind::Mirror => Pattern::Mirror { axes: [true, false, false], centre: false },
            PatternKind::Rect => Pattern::Rect { counts: [3, 1, 1], spacing: [40.0, 40.0, 40.0] },
            PatternKind::Circular => Pattern::Circular { count: 6, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: true },
            PatternKind::Path => Pattern::Path { source: 0, count: 8, mode: PathMode::Even, step: 20.0, align: true },
            PatternKind::Geometry => Pattern::Geometry { source: Source::Shape(0), orient: false },
        }
    }

    /// Keep every number in a range the renderer copes with.
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        match self {
            Pattern::Mirror { .. } | Pattern::Geometry { .. } => {}
            Pattern::Rect { counts, spacing } => {
                for c in counts.iter_mut() {
                    *c = (*c).clamp(1, MAX_SIDE);
                }
                for s in spacing.iter_mut() {
                    fix(s, -COORD, COORD, 40.0);
                }
            }
            Pattern::Circular { count, angle, axis, centre, .. } => {
                *count = (*count).clamp(1, MAX_INSTANCES as u32);
                fix(angle, -3600.0, 3600.0, 360.0);
                *axis = (*axis).min(2);
                for c in centre.iter_mut() {
                    fix(c, -COORD, COORD, 0.0);
                }
            }
            Pattern::Path { count, step, .. } => {
                *count = (*count).clamp(1, MAX_INSTANCES as u32);
                fix(step, 0.1, COORD, 20.0);
            }
        }
    }

    /// The `uid` of the shape or group this pattern follows, if it follows one.
    pub fn follows(&self) -> Option<Source> {
        match self {
            Pattern::Path { source, .. } => Some(Source::Shape(*source)),
            Pattern::Geometry { source, .. } => Some(*source),
            _ => None,
        }
    }
}

/// How a group is placed when it is a linked copy: moved, turned (degrees) and scaled about the
/// middle of the source.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Xform {
    pub pos: V3,
    pub rot: V3,
    pub scale: f32,
}

impl Default for Xform {
    fn default() -> Self {
        Xform { pos: [0.0; 3], rot: [0.0; 3], scale: 1.0 }
    }
}

impl Xform {
    pub fn is_identity(&self) -> bool {
        *self == Xform::default()
    }
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, lo: f32, hi: f32, d: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { d };
        };
        for v in &mut self.pos {
            fix(v, -COORD, COORD, 0.0);
        }
        for v in &mut self.rot {
            fix(v, -3600.0, 3600.0, 0.0);
        }
        fix(&mut self.scale, 0.01, 100.0, 1.0);
    }
    /// The instance transform about the centre `c` of the source.
    pub fn inst(&self, c: V3) -> Inst {
        let m = rotation(self.rot);
        let s = if self.scale.is_finite() { self.scale.clamp(0.01, 100.0) } else { 1.0 };
        Inst { m, t: add(sub(c, mul(mat_vec(&m, c), s)), self.pos), s }
    }
}

// ---------------------------------------------------------------------------------------------
// Where the points of a path or a geometry pattern are

/// A line of points in the world, open or closed.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub pts: Vec<V3>,
    pub closed: bool,
}

fn world_of(s: &Shape, q: V3) -> V3 {
    let r = rotation(s.rot);
    add(s.pos, mat_vec(&r, [q[0] * s.scale[0], q[1] * s.scale[1], q[2] * s.scale[2]]))
}

/// The path a shape draws, in the world: a curve's smooth line, a sweep's nodes, an outline's
/// edges. `None` for solids that have none.
pub fn shape_polyline(s: &Shape) -> Option<Poly> {
    let to_world = |pts: Vec<V3>, closed: bool| {
        let pts: Vec<V3> = pts.into_iter().map(|q| world_of(s, q)).filter(|p| p.iter().all(|v| v.is_finite())).collect();
        (pts.len() >= 2).then_some(Poly { pts, closed })
    };
    match s.kind {
        Kind::Curve => {
            let (closed, ..) = crate::curve::settings(&s.params);
            to_world(crate::curve::build(&s.path, &s.params).iter().map(|p| [p[0], p[1], p[2]]).collect(), closed)
        }
        Kind::Sweep => to_world(s.path.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| [p[0], p[1], p[2]]).collect(), false),
        Kind::Extrude | Kind::Loft => to_world(s.outline.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| [p[0], p[1], 0.0]).collect(), true),
        Kind::Revolve => to_world(s.outline.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| [p[0].max(0.0), 0.0, p[1]]).collect(), true),
        _ => None,
    }
}

/// The points a shape offers a geometry pattern, with the direction along the geometry at each
/// (when it is a line).
fn shape_points(s: &Shape) -> Vec<(V3, Option<V3>)> {
    let nodes: Option<(Vec<V3>, bool)> = match s.kind {
        Kind::Curve | Kind::Sweep => Some((s.path.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| world_of(s, [p[0], p[1], p[2]])).collect(), s.kind == Kind::Curve && crate::curve::settings(&s.params).0)),
        Kind::Extrude | Kind::Loft => Some((s.outline.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| world_of(s, [p[0], p[1], 0.0])).collect(), true)),
        Kind::Revolve => Some((s.outline.iter().filter(|p| p.iter().all(|v| v.is_finite())).map(|p| world_of(s, [p[0].max(0.0), 0.0, p[1]])).collect(), true)),
        _ => None,
    };
    if let Some((pts, closed)) = nodes {
        let n = pts.len();
        return (0..n)
            .map(|i| {
                let prev = if i > 0 { pts.get(i - 1) } else if closed { pts.get(n - 1) } else { None };
                let next = if i + 1 < n { pts.get(i + 1) } else if closed { pts.first() } else { None };
                let here = pts.get(i).copied().unwrap_or([0.0; 3]);
                let tangent = match (prev, next) {
                    (Some(a), Some(b)) => Some(sub(*b, *a)),
                    (None, Some(b)) => Some(sub(*b, here)),
                    (Some(a), None) => Some(sub(here, *a)),
                    (None, None) => None,
                };
                (here, tangent.filter(|t| len(*t) > 1e-6).map(norm))
            })
            .collect();
    }
    match s.kind {
        // the eight corners of the box
        Kind::Box | Kind::Wedge => {
            let h = [s.size[0] * 0.5, s.size[1] * 0.5, s.size[2] * 0.5];
            let mut v = Vec::new();
            for i in 0..8u32 {
                let q = [if i & 1 == 0 { -h[0] } else { h[0] }, if i & 2 == 0 { -h[1] } else { h[1] }, if i & 4 == 0 { -h[2] } else { h[2] }];
                v.push((world_of(s, q), None));
            }
            v
        }
        _ => vec![(s.pos, None)],
    }
}

/// Length of the line, and a point and direction `d` mm from its start (clamped at the ends; a
/// closed line goes round).
fn along(poly: &Poly, d: f32) -> (V3, V3) {
    let n = poly.pts.len();
    if n == 0 {
        return ([0.0; 3], [1.0, 0.0, 0.0]);
    }
    let seg = |i: usize| -> (V3, V3) {
        let a = poly.pts.get(i % n).copied().unwrap_or([0.0; 3]);
        let b = poly.pts.get((i + 1) % n).copied().unwrap_or(a);
        (a, b)
    };
    let segs = if poly.closed { n } else { n.saturating_sub(1) };
    let mut left = d.max(0.0);
    let mut last = ([0.0; 3], [1.0, 0.0, 0.0]);
    for i in 0..segs {
        let (a, b) = seg(i);
        let l = len(sub(b, a));
        let dir = if l > 1e-9 { mul(sub(b, a), 1.0 / l) } else { last.1 };
        last = (b, dir);
        if left <= l || i + 1 == segs {
            let t = if l > 1e-9 { (left / l).clamp(0.0, 1.0) } else { 0.0 };
            return (add(a, mul(sub(b, a), t)), dir);
        }
        left -= l;
    }
    last
}

/// The length of the line.
pub fn poly_length(poly: &Poly) -> f32 {
    let n = poly.pts.len();
    let segs = if poly.closed { n } else { n.saturating_sub(1) };
    (0..segs)
        .map(|i| {
            let a = poly.pts.get(i).copied().unwrap_or([0.0; 3]);
            let b = poly.pts.get((i + 1) % n.max(1)).copied().unwrap_or(a);
            len(sub(b, a))
        })
        .sum()
}

// ---------------------------------------------------------------------------------------------
// Making the instances

/// The middle of a group's visible shapes.
pub fn centre_of_shapes(shapes: &[Shape]) -> V3 {
    let vis: Vec<&Shape> = shapes.iter().filter(|s| s.visible).collect();
    if vis.is_empty() {
        return [0.0; 3];
    }
    let n = vis.len() as f32;
    let c = vis.iter().fold([0.0; 3], |a, s| add(a, s.pos));
    let c = mul(c, 1.0 / n);
    if c.iter().all(|v| v.is_finite()) { c } else { [0.0; 3] }
}

/// The points and directions a pattern follows, found in the scene.
fn follow_points(src: Source, scene: &Scene) -> Vec<(V3, Option<V3>, Option<Mat>)> {
    match src {
        Source::Shape(uid) => match scene.shape_by_uid(uid).and_then(|(g, s)| scene.shape(g, s)) {
            Some(s) => {
                let rot = rotation(s.rot);
                shape_points(s).into_iter().map(|(p, t)| (p, t, Some(rot))).collect()
            }
            None => Vec::new(),
        },
        Source::Group(uid) => match scene.group_by_uid(uid).and_then(|g| scene.groups.get(g)) {
            Some(g) => g.shapes.iter().filter(|s| s.visible).map(|s| (s.pos, None, None)).collect(),
            None => Vec::new(),
        },
    }
}

/// How many instances the pattern would make before the scene's limit (cheap: no instances are made).
pub fn natural_count(p: &Pattern, scene: &Scene) -> usize {
    let mut p = p.clone();
    p.sanitize();
    match &p {
        Pattern::Mirror { axes, .. } => 1usize << axes.iter().filter(|a| **a).count(),
        Pattern::Rect { counts, .. } => counts.iter().map(|c| *c as usize).product(),
        Pattern::Circular { count, .. } => *count as usize,
        Pattern::Path { source, count, mode, step, .. } => {
            let Some(poly) = scene.shape_by_uid(*source).and_then(|(g, s)| scene.shape(g, s)).and_then(shape_polyline) else { return 0 };
            match mode {
                PathMode::Even => *count as usize,
                PathMode::Step => (((poly_length(&poly) / step.max(0.1)).floor() as usize) + 1).min(*count as usize),
            }
        }
        Pattern::Geometry { source, .. } => follow_points(*source, scene).len(),
    }
}

/// The instances of a pattern, at most `room`. `anchor` is the middle of the group: path and
/// geometry patterns put it on each point. Nothing at all means the pattern has nothing to follow
/// (the group then shows once, as it is).
pub fn generate(p: &Pattern, scene: &Scene, anchor: V3, room: usize) -> Vec<Inst> {
    let mut p = p.clone();
    p.sanitize();
    let room = room.clamp(1, MAX_INSTANCES);
    let mut out: Vec<Inst> = Vec::new();
    match &p {
        Pattern::Mirror { axes, centre } => {
            let c = if *centre { anchor } else { [0.0; 3] };
            let used: Vec<usize> = (0..3).filter(|a| axes.get(*a).copied().unwrap_or(false)).collect();
            for mask in 0..(1usize << used.len()) {
                let mut m = EYE;
                for (bit, a) in used.iter().enumerate() {
                    if mask >> bit & 1 == 1 {
                        m[*a][*a] = -1.0;
                    }
                }
                out.push(Inst::about(m, c));
            }
        }
        Pattern::Rect { counts, spacing } => {
            'grid: for k in 0..counts[2] {
                for j in 0..counts[1] {
                    for i in 0..counts[0] {
                        out.push(Inst::moved([spacing[0] * i as f32, spacing[1] * j as f32, spacing[2] * k as f32]));
                        if out.len() >= room {
                            break 'grid;
                        }
                    }
                }
            }
        }
        Pattern::Circular { count, angle, axis, centre, rotate } => {
            let n = (*count as usize).min(room);
            let per = if angle.abs() >= 359.5 { *angle / *count as f32 } else { *angle / (count.saturating_sub(1)).max(1) as f32 };
            for k in 0..n {
                let m = axis_turn(*axis, per * k as f32);
                if *rotate {
                    out.push(Inst::about(m, *centre));
                } else {
                    // the group's middle goes round, the group keeps its direction
                    let moved = add(*centre, mat_vec(&m, sub(anchor, *centre)));
                    out.push(Inst::moved(sub(moved, anchor)));
                }
            }
        }
        Pattern::Path { source, count, mode, step, align } => {
            let Some(poly) = scene.shape_by_uid(*source).and_then(|(g, s)| scene.shape(g, s)).and_then(shape_polyline) else { return out };
            let total = poly_length(&poly);
            let n = (*count as usize).min(room);
            let dists: Vec<f32> = match mode {
                PathMode::Even => {
                    let gaps = if poly.closed { n } else { n.saturating_sub(1) }.max(1);
                    (0..n).map(|k| total * k as f32 / gaps as f32).collect()
                }
                PathMode::Step => (0..n).map(|k| *step * k as f32).take_while(|d| *d <= total + 1e-3).collect(),
            };
            for d in dists {
                let (pt, tan) = along(&poly, d);
                out.push(place_at(pt, anchor, if *align { Some(frame_along(tan)) } else { None }));
            }
        }
        Pattern::Geometry { source, orient } => {
            for (pt, tan, rot) in follow_points(*source, scene).into_iter().take(room) {
                let frame = if *orient { tan.map(frame_along).or(rot) } else { None };
                out.push(place_at(pt, anchor, frame));
            }
        }
    }
    out.retain(Inst::finite);
    out
}

/// The instance that puts the group's middle `anchor` on `pt`, turned by `frame` about it.
fn place_at(pt: V3, anchor: V3, frame: Option<Mat>) -> Inst {
    let m = frame.unwrap_or(EYE);
    Inst { m, t: sub(pt, mat_vec(&m, anchor)), s: 1.0 }
}

// ---------------------------------------------------------------------------------------------
// The scene's groups, resolved

/// One visible group as it is drawn: which group's shapes it shows and through which instances.
#[derive(Clone, Debug, PartialEq)]
pub struct RGroup {
    /// The group (its op, blend, visibility and name).
    pub gi: usize,
    /// The group whose shapes are shown (the same group, or the source of a linked copy).
    pub src: usize,
    /// Empty = once, as it is.
    pub insts: Vec<Inst>,
    pub linked: bool,
    /// Some instances were left out to stay under `MAX_INSTANCES`.
    pub capped: bool,
}

/// The source group of a linked copy, following links up to a few steps (a loop ends it).
pub fn link_source(scene: &Scene, gi: usize) -> Option<usize> {
    let mut at = gi;
    for _ in 0..4 {
        let uid = scene.groups.get(at)?.instance_of?;
        let next = scene.group_by_uid(uid)?;
        if next == at {
            return None;
        }
        at = next;
        if scene.groups.get(at)?.instance_of.is_none() {
            return Some(at);
        }
    }
    None
}

/// The groups to draw, in order, with their instances. Hidden groups are left out; a linked copy
/// whose source is gone is left out too.
pub fn resolve(scene: &Scene) -> Vec<RGroup> {
    let mut left = MAX_INSTANCES;
    let mut out = Vec::new();
    for (gi, g) in scene.groups.iter().enumerate() {
        if !g.visible {
            continue;
        }
        let linked = g.instance_of.is_some();
        let src = if linked {
            match link_source(scene, gi) {
                Some(s) => s,
                None => continue,
            }
        } else {
            gi
        };
        let src_shapes: &[Shape] = scene.groups.get(src).map_or(&[], |s| &s.shapes);
        let centre = centre_of_shapes(src_shapes);
        let own = if linked { Some(g.xform.inst(centre)) } else { None };
        let anchor = own.map_or(centre, |o| o.apply(centre));
        let wants_pattern = g.pattern.is_some();
        if !linked && !wants_pattern {
            out.push(RGroup { gi, src, insts: Vec::new(), linked: false, capped: false });
            continue;
        }
        let natural = g.pattern.as_ref().map_or(1, |p| natural_count(p, scene).max(1));
        let mut insts: Vec<Inst> = match &g.pattern {
            Some(p) => generate(p, scene, anchor, left.max(1)),
            None => Vec::new(),
        };
        if insts.is_empty() {
            insts.push(Inst::IDENTITY);
        }
        let capped = natural > insts.len();
        // a linked copy's own placement comes first, then the pattern's
        if let Some(o) = own {
            insts = insts.iter().map(|a| o.then(a)).collect();
        }
        left = left.saturating_sub(insts.len());
        if !linked && insts.len() == 1 && insts[0] == Inst::IDENTITY {
            insts.clear();
        }
        out.push(RGroup { gi, src, insts, linked, capped });
    }
    out
}

/// The instance count to show in the inspector: (how many are drawn, whether some were left out).
pub fn describe(scene: &Scene, gi: usize) -> (usize, bool) {
    match resolve(scene).into_iter().find(|r| r.gi == gi) {
        Some(r) => (r.insts.len().max(1), r.capped),
        None => (0, false),
    }
}

/// A sphere around the whole of a visible group's `Add` parts, in the group's own space.
pub fn group_sphere(shapes: &[Shape]) -> Bound {
    let spheres: Vec<Bound> = shapes.iter().filter(|s| s.visible).map(crate::accel::shape_bound).collect();
    crate::accel::enclose(&spheres)
}

// ---------------------------------------------------------------------------------------------

impl Scene {
    /// The group with this `uid` (0 is no group).
    pub fn group_by_uid(&self, uid: u32) -> Option<usize> {
        if uid == 0 {
            return None;
        }
        self.groups.iter().position(|g| g.uid == uid)
    }
    /// The (group, shape) with this `uid` (0 is no shape).
    pub fn shape_by_uid(&self, uid: u32) -> Option<(usize, usize)> {
        if uid == 0 {
            return None;
        }
        self.groups.iter().enumerate().find_map(|(gi, g)| g.shapes.iter().position(|s| s.uid == uid).map(|si| (gi, si)))
    }
    fn max_uid(&self) -> u32 {
        self.groups.iter().flat_map(|g| g.shapes.iter().map(|s| s.uid).chain(std::iter::once(g.uid))).max().unwrap_or(0)
    }
    /// The `uid` of a group, given one first if it had none.
    pub fn ensure_group_uid(&mut self, gi: usize) -> Option<u32> {
        let fresh = self.max_uid().saturating_add(1);
        let g = self.groups.get_mut(gi)?;
        if g.uid == 0 {
            g.uid = fresh;
        }
        Some(g.uid)
    }
    /// The `uid` of a shape, given one first if it had none.
    pub fn ensure_shape_uid(&mut self, gi: usize, si: usize) -> Option<u32> {
        let fresh = self.max_uid().saturating_add(1);
        let s = self.groups.get_mut(gi)?.shapes.get_mut(si)?;
        if s.uid == 0 {
            s.uid = fresh;
        }
        Some(s.uid)
    }
    /// A copied group or shape carries its original's `uid`: the first one in the list keeps it,
    /// later ones lose it (and so are not what a pattern follows).
    pub fn dedupe_uids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for g in &mut self.groups {
            if g.uid != 0 && !seen.insert((0u8, g.uid)) {
                g.uid = 0;
            }
            for s in &mut g.shapes {
                if s.uid != 0 && !seen.insert((1u8, s.uid)) {
                    s.uid = 0;
                }
            }
        }
    }
}

impl Group {
    /// Is it a linked copy of another group?
    pub fn is_linked(&self) -> bool {
        self.instance_of.is_some()
    }
}

/// Reading a pattern from a file never fails the whole file: a pattern we do not know is dropped.
pub fn lenient_pattern<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Pattern>, D::Error> {
    let v: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(v.and_then(|v| serde_json::from_value::<Pattern>(v).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Material;

    fn close(a: V3, b: V3) {
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-3), "{a:?} != {b:?}");
    }

    fn ball(p: V3) -> Shape {
        Shape::new(Kind::Sphere, p, Material::default())
    }

    fn scene_with(shapes: Vec<Shape>, pattern: Option<Pattern>) -> Scene {
        let mut g = Group::new("g");
        g.shapes = shapes;
        g.pattern = pattern;
        Scene { groups: vec![g], ..Scene::default() }
    }

    #[test]
    fn an_instance_carries_points_there_and_back() {
        let i = Inst { m: axis_turn(2, 90.0), t: [5.0, -2.0, 1.0], s: 2.0 };
        let q = [1.0, 2.0, 3.0];
        let w = i.apply(q);
        close(i.back(w), q);
        // the rows the shader reads do the same
        let r = i.back_rows();
        let back = [0, 1, 2].map(|k| r[k][0] * w[0] + r[k][1] * w[1] + r[k][2] * w[2] + r[k][3]);
        close(back, q);
        // a quarter turn about Z takes +X to +Y
        close(Inst::about(axis_turn(2, 90.0), [0.0; 3]).apply([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
        // turning about a point leaves that point where it is
        close(Inst::about(axis_turn(2, 33.0), [4.0, 5.0, 6.0]).apply([4.0, 5.0, 6.0]), [4.0, 5.0, 6.0]);
        // first this, then that
        let a = Inst::moved([1.0, 0.0, 0.0]);
        let b = Inst { m: axis_turn(2, 90.0), t: [0.0; 3], s: 1.0 };
        close(a.then(&b).apply([0.0; 3]), b.apply(a.apply([0.0; 3])));
        assert!(!a.flips());
    }

    #[test]
    fn a_mirror_flips_across_the_chosen_planes() {
        let sc = scene_with(vec![ball([10.0, 4.0, 2.0])], None);
        let one = generate(&Pattern::Mirror { axes: [true, false, false], centre: false }, &sc, [10.0, 4.0, 2.0], 256);
        assert_eq!(one.len(), 2);
        close(one[0].apply([10.0, 4.0, 2.0]), [10.0, 4.0, 2.0]);
        close(one[1].apply([10.0, 4.0, 2.0]), [-10.0, 4.0, 2.0]);
        assert!(one[1].flips() && !one[0].flips());
        let two = generate(&Pattern::Mirror { axes: [true, true, false], centre: false }, &sc, [0.0; 3], 256);
        assert_eq!(two.len(), 4);
        let three = generate(&Pattern::Mirror { axes: [true, true, true], centre: false }, &sc, [0.0; 3], 256);
        assert_eq!(three.len(), 8);
        let mut at: Vec<V3> = three.iter().map(|i| i.apply([1.0, 2.0, 3.0])).collect();
        at.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        at.dedup_by(|a, b| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-4));
        assert_eq!(at.len(), 8, "eight different corners");
        // about the group's own middle: the plane goes through it
        let own = generate(&Pattern::Mirror { axes: [true, false, false], centre: true }, &sc, [10.0, 4.0, 2.0], 256);
        close(own[1].apply([10.0, 4.0, 2.0]), [10.0, 4.0, 2.0]);
        close(own[1].apply([13.0, 4.0, 2.0]), [7.0, 4.0, 2.0]);
        // no axis: one copy as it is
        assert_eq!(generate(&Pattern::Mirror { axes: [false; 3], centre: false }, &sc, [0.0; 3], 256).len(), 1);
    }

    #[test]
    fn a_rectangular_pattern_counts_and_places() {
        let sc = scene_with(vec![ball([0.0; 3])], None);
        let p = Pattern::Rect { counts: [3, 2, 2], spacing: [10.0, 20.0, 30.0] };
        let v = generate(&p, &sc, [0.0; 3], 256);
        assert_eq!(v.len(), 12);
        assert_eq!(natural_count(&p, &sc), 12);
        close(v[0].t, [0.0; 3]);
        close(v[1].t, [10.0, 0.0, 0.0]);
        close(v[3].t, [0.0, 20.0, 0.0]);
        close(v[11].t, [20.0, 20.0, 30.0]);
        // negative spacing goes the other way; the room limits the count
        let n = generate(&Pattern::Rect { counts: [4, 1, 1], spacing: [-5.0, 0.0, 0.0] }, &sc, [0.0; 3], 3);
        assert_eq!(n.len(), 3);
        close(n[2].t, [-10.0, 0.0, 0.0]);
    }

    #[test]
    fn a_circular_pattern_turns_about_its_axis() {
        let sc = scene_with(vec![ball([20.0, 0.0, 0.0])], None);
        let p = Pattern::Circular { count: 4, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: true };
        let v = generate(&p, &sc, [20.0, 0.0, 0.0], 256);
        assert_eq!(v.len(), 4);
        close(v[1].apply([20.0, 0.0, 0.0]), [0.0, 20.0, 0.0]);
        close(v[2].apply([20.0, 0.0, 0.0]), [-20.0, 0.0, 0.0]);
        close(v[3].apply([20.0, 0.0, 0.0]), [0.0, -20.0, 0.0]);
        // part of a turn ends on its last copy
        let q = Pattern::Circular { count: 3, angle: 180.0, axis: 2, centre: [0.0; 3], rotate: true };
        let w = generate(&q, &sc, [20.0, 0.0, 0.0], 256);
        close(w[2].apply([20.0, 0.0, 0.0]), [-20.0, 0.0, 0.0]);
        // the copies keep their direction when `rotate` is off
        let keep = Pattern::Circular { count: 4, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: false };
        let k = generate(&keep, &sc, [20.0, 0.0, 0.0], 256);
        close(k[1].apply([20.0, 0.0, 0.0]), [0.0, 20.0, 0.0]);
        close(k[1].apply([21.0, 0.0, 0.0]), [1.0, 20.0, 0.0]);
        assert_eq!(k[1].m, EYE);
        // about the X axis through a point
        let x = Pattern::Circular { count: 2, angle: 360.0, axis: 0, centre: [0.0, 10.0, 0.0], rotate: true };
        let r = generate(&x, &sc, [0.0; 3], 256);
        close(r[1].apply([0.0, 10.0, 5.0]), [0.0, 10.0, -5.0]);
    }

    fn curve_scene() -> Scene {
        let mut c = Shape::new(Kind::Sweep, [0.0, 0.0, 0.0], Material::default());
        c.path = vec![[0.0, 0.0, 0.0, 1.0], [100.0, 0.0, 0.0, 1.0]];
        let mut sc = scene_with(vec![ball([0.0; 3]), c], None);
        sc.ensure_shape_uid(0, 1);
        sc
    }

    #[test]
    fn a_path_pattern_spaces_copies_along_the_line() {
        let mut sc = curve_scene();
        let uid = sc.groups[0].shapes[1].uid;
        assert!(uid != 0);
        let even = Pattern::Path { source: uid, count: 5, mode: PathMode::Even, step: 10.0, align: false };
        let v = generate(&even, &sc, [0.0; 3], 256);
        assert_eq!(v.len(), 5);
        for (k, i) in v.iter().enumerate() {
            close(i.apply([0.0; 3]), [25.0 * k as f32, 0.0, 0.0]);
        }
        // by distance
        let step = Pattern::Path { source: uid, count: 50, mode: PathMode::Step, step: 30.0, align: false };
        let s = generate(&step, &sc, [0.0; 3], 256);
        assert_eq!(s.len(), 4, "0, 30, 60, 90");
        assert_eq!(natural_count(&step, &sc), 4);
        close(s[3].apply([0.0; 3]), [90.0, 0.0, 0.0]);
        // aligned: the group's X axis follows the line (here the line is along X: no turn)
        let al = Pattern::Path { source: uid, count: 2, mode: PathMode::Even, step: 10.0, align: true };
        let a = generate(&al, &sc, [0.0; 3], 256);
        close(a[1].apply([1.0, 0.0, 0.0]), [101.0, 0.0, 0.0]);
        // a path along Y turns the copies a quarter turn
        sc.groups[0].shapes[1].rot = [0.0, 0.0, 90.0];
        let up = generate(&al, &sc, [0.0; 3], 256);
        close(up[1].apply([1.0, 0.0, 0.0]), [0.0, 101.0, 0.0]);
        // a missing source makes nothing (the group then shows once)
        let none = Pattern::Path { source: 999, count: 5, mode: PathMode::Even, step: 10.0, align: false };
        assert!(generate(&none, &sc, [0.0; 3], 256).is_empty());
        assert_eq!(natural_count(&none, &sc), 0);
        // a closed outline goes round: the last copy is not on the first
        let mut o = Shape::new(Kind::Extrude, [0.0; 3], Material::default());
        o.outline = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let mut sc2 = scene_with(vec![ball([0.0; 3]), o], None);
        let ouid = sc2.ensure_shape_uid(0, 1).unwrap_or(0);
        let ring = generate(&Pattern::Path { source: ouid, count: 4, mode: PathMode::Even, step: 10.0, align: false }, &sc2, [0.0; 3], 256);
        assert_eq!(ring.len(), 4);
        close(ring[1].apply([0.0; 3]), [10.0, 0.0, 0.0]);
        close(ring[2].apply([0.0; 3]), [10.0, 10.0, 0.0]);
        close(ring[3].apply([0.0; 3]), [0.0, 10.0, 0.0]);
    }

    #[test]
    fn a_geometry_pattern_puts_a_copy_on_every_point() {
        let mut sc = curve_scene();
        let outline_uid = {
            let mut o = Shape::new(Kind::Extrude, [100.0, 0.0, 0.0], Material::default());
            o.outline = vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0]];
            sc.groups[0].shapes.push(o);
            sc.ensure_shape_uid(0, 2).unwrap_or(0)
        };
        let g = Pattern::Geometry { source: Source::Shape(outline_uid), orient: false };
        let v = generate(&g, &sc, [0.0; 3], 256);
        assert_eq!(v.len(), 3);
        close(v[0].apply([0.0; 3]), [95.0, -5.0, 0.0]);
        close(v[2].apply([0.0; 3]), [105.0, 5.0, 0.0]);
        // a box offers its eight corners, a sphere its middle
        let mut b = Shape::new(Kind::Box, [0.0, 0.0, 0.0], Material::default());
        b.size = [2.0, 4.0, 6.0];
        sc.groups[0].shapes.push(b);
        let buid = sc.ensure_shape_uid(0, 3).unwrap_or(0);
        assert_eq!(generate(&Pattern::Geometry { source: Source::Shape(buid), orient: false }, &sc, [0.0; 3], 256).len(), 8);
        sc.ensure_shape_uid(0, 0);
        let suid = sc.groups[0].shapes[0].uid;
        assert_eq!(generate(&Pattern::Geometry { source: Source::Shape(suid), orient: false }, &sc, [0.0; 3], 256).len(), 1);
        // a group offers the middle of each of its shapes
        let guid = sc.ensure_group_uid(0).unwrap_or(0);
        assert_eq!(generate(&Pattern::Geometry { source: Source::Group(guid), orient: false }, &sc, [0.0; 3], 256).len(), 4);
        // oriented along a line: copies turn with the line's direction
        let sw = Pattern::Geometry { source: Source::Shape(sc.groups[0].shapes[1].uid), orient: true };
        let o = generate(&sw, &sc, [0.0; 3], 256);
        assert_eq!(o.len(), 2);
        close(o[1].apply([1.0, 0.0, 0.0]), [101.0, 0.0, 0.0]);
        // the room limits it
        assert_eq!(generate(&Pattern::Geometry { source: Source::Shape(buid), orient: false }, &sc, [0.0; 3], 5).len(), 5);
    }

    #[test]
    fn hostile_numbers_make_safe_patterns() {
        let sc = curve_scene();
        let mut worst = vec![
            Pattern::Rect { counts: [u32::MAX, u32::MAX, u32::MAX], spacing: [f32::NAN, f32::INFINITY, 1e30] },
            Pattern::Circular { count: u32::MAX, angle: f32::NAN, axis: 200, centre: [f32::INFINITY, 0.0, f32::NAN], rotate: true },
            Pattern::Path { source: u32::MAX, count: 0, mode: PathMode::Step, step: f32::NAN, align: true },
            Pattern::Path { source: sc.groups[0].shapes[1].uid, count: u32::MAX, mode: PathMode::Step, step: -4.0, align: true },
            Pattern::Geometry { source: Source::Group(u32::MAX), orient: true },
            Pattern::Mirror { axes: [true; 3], centre: true },
        ];
        for p in &mut worst {
            let v = generate(p, &sc, [f32::NAN, 0.0, 1e30], 1_000_000);
            assert!(v.len() <= MAX_INSTANCES, "{p:?} made {}", v.len());
            assert!(v.iter().all(Inst::finite), "{p:?}");
            assert!(natural_count(p, &sc) <= 1 << 20);
            p.sanitize();
            let again = serde_json::to_string(p).unwrap_or_default();
            assert!(!again.contains("null") && !again.contains("NaN"), "{again}");
        }
        // a hostile link transform
        let mut x = Xform { pos: [f32::NAN; 3], rot: [1e30; 3], scale: f32::INFINITY };
        x.sanitize();
        assert!(x.scale <= 100.0 && x.pos.iter().all(|v| v.is_finite()) && x.inst([0.0; 3]).finite());
    }

    #[test]
    fn a_pattern_group_resolves_to_its_instances_and_stays_under_the_limit() {
        let mut sc = scene_with(vec![ball([0.0; 3])], Some(Pattern::Rect { counts: [4, 3, 1], spacing: [30.0, 30.0, 0.0] }));
        let r = resolve(&sc);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].insts.len(), 12);
        assert!(!r[0].capped);
        assert_eq!(describe(&sc, 0), (12, false));
        // a plain group has no instances; a hidden one is not drawn
        sc.groups.push(Group::new("plain"));
        sc.groups[1].shapes.push(ball([0.0; 3]));
        assert!(resolve(&sc)[1].insts.is_empty());
        sc.groups[1].visible = false;
        assert_eq!(resolve(&sc).len(), 1);
        // too many: later groups get what is left, never nothing
        let mut big = scene_with(vec![ball([0.0; 3])], Some(Pattern::Rect { counts: [20, 10, 1], spacing: [30.0, 30.0, 0.0] }));
        let mut second = big.groups[0].clone();
        second.name = "second".into();
        big.groups.push(second);
        let r = resolve(&big);
        let total: usize = r.iter().map(|g| g.insts.len().max(1)).sum();
        assert!(total <= MAX_INSTANCES + 1, "{total}");
        assert!(!r[0].capped && r[0].insts.len() == 200);
        assert!(r[1].capped && r[1].insts.len() == 56, "the second group gets what is left: {}", r[1].insts.len());
        // every group gets at least one instance even when the room is gone
        let many = 40;
        let mut crowd = scene_with(vec![ball([0.0; 3])], Some(Pattern::Rect { counts: [10, 1, 1], spacing: [30.0, 0.0, 0.0] }));
        let one = crowd.groups[0].clone();
        for _ in 0..many {
            crowd.groups.push(one.clone());
        }
        let r = resolve(&crowd);
        assert_eq!(r.len(), many + 1);
        assert!(r.iter().enumerate().all(|(i, g)| g.gi == i), "every group is still drawn once the room is gone");
        assert!(r.iter().all(|g| g.insts.len() <= 10));
    }

    #[test]
    fn a_linked_copy_shows_the_source_through_its_own_placement() {
        let mut sc = scene_with(vec![ball([10.0, 0.0, 0.0]), ball([30.0, 0.0, 0.0])], None);
        let uid = sc.ensure_group_uid(0).unwrap_or(0);
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [0.0, 100.0, 0.0], rot: [0.0; 3], scale: 1.0 };
        sc.groups.push(copy);
        let r = resolve(&sc);
        assert_eq!(r.len(), 2);
        assert_eq!((r[1].gi, r[1].src, r[1].linked), (1, 0, true));
        assert_eq!(r[1].insts.len(), 1);
        close(r[1].insts[0].apply([10.0, 0.0, 0.0]), [10.0, 100.0, 0.0]);
        // turned about the middle of the source (20, 0, 0)
        sc.groups[1].xform = Xform { pos: [0.0; 3], rot: [0.0, 0.0, 180.0], scale: 1.0 };
        let r = resolve(&sc);
        close(r[1].insts[0].apply([10.0, 0.0, 0.0]), [30.0, 0.0, 0.0]);
        // scaled about the middle
        sc.groups[1].xform = Xform { pos: [0.0; 3], rot: [0.0; 3], scale: 2.0 };
        let r = resolve(&sc);
        close(r[1].insts[0].apply([10.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        assert_eq!(r[1].insts[0].s, 2.0);
        // a copy with a pattern of its own: placement first, then each instance
        sc.groups[1].xform = Xform { pos: [0.0, 100.0, 0.0], ..Xform::default() };
        sc.groups[1].pattern = Some(Pattern::Rect { counts: [1, 1, 2], spacing: [0.0, 0.0, 50.0] });
        let r = resolve(&sc);
        assert_eq!(r[1].insts.len(), 2);
        close(r[1].insts[1].apply([10.0, 0.0, 0.0]), [10.0, 100.0, 50.0]);
        // a copy of a missing group is not drawn; links to links resolve to the root; a loop ends
        sc.groups[1].instance_of = Some(777);
        assert_eq!(resolve(&sc).len(), 1);
        sc.groups[1].instance_of = Some(uid);
        let mut third = Group::new("third");
        third.uid = 5;
        sc.groups[1].uid = 4;
        third.instance_of = Some(4);
        sc.groups.push(third);
        assert_eq!(link_source(&sc, 2), Some(0));
        sc.groups[0].instance_of = Some(5);
        assert_eq!(link_source(&sc, 2), None, "a loop of links draws nothing");
        sc.groups[0].instance_of = Some(uid);
        assert_eq!(link_source(&sc, 0), None, "a group that links to itself draws nothing");
    }

    #[test]
    fn uids_find_things_and_copies_lose_them() {
        let mut sc = scene_with(vec![ball([0.0; 3]), ball([1.0; 3])], None);
        let a = sc.ensure_shape_uid(0, 1).unwrap_or(0);
        let g = sc.ensure_group_uid(0).unwrap_or(0);
        assert!(a != 0 && g != 0);
        assert_eq!(sc.ensure_shape_uid(0, 1), Some(a), "asking again gives the same one");
        assert_eq!(sc.shape_by_uid(a), Some((0, 1)));
        assert_eq!(sc.group_by_uid(g), Some(0));
        assert_eq!(sc.shape_by_uid(0), None);
        assert_eq!(sc.shape_by_uid(12345), None);
        // a clone carries the uid; the first one keeps it
        let dup = sc.groups[0].shapes[1].clone();
        sc.groups[0].shapes.push(dup);
        let copy = sc.groups[0].clone();
        sc.groups.push(copy);
        sc.dedupe_uids();
        assert_eq!(sc.groups[0].shapes[2].uid, 0);
        assert_eq!(sc.groups[1].uid, 0);
        assert_eq!(sc.groups[1].shapes[1].uid, 0);
        assert_eq!(sc.shape_by_uid(a), Some((0, 1)));
        // a fresh uid is never one that exists
        let b = sc.ensure_shape_uid(0, 2).unwrap_or(0);
        assert!(b != 0 && b != a && b != g);
        // deleting the target just makes the reference find nothing
        sc.groups[0].shapes.remove(1);
        assert_eq!(sc.shape_by_uid(a), None);
    }

    #[test]
    fn patterns_round_trip_and_old_files_and_unknown_patterns_open() {
        let mut sc = scene_with(vec![ball([0.0; 3])], Some(Pattern::Circular { count: 5, angle: 270.0, axis: 1, centre: [1.0, 2.0, 3.0], rotate: false }));
        sc.ensure_group_uid(0);
        sc.groups[0].xform = Xform { pos: [1.0, 2.0, 3.0], rot: [4.0, 5.0, 6.0], scale: 1.5 };
        sc.groups[0].instance_of = Some(9);
        let back = Scene::from_json(&sc.to_json()).unwrap_or_default();
        assert_eq!(back.groups[0].pattern, sc.groups[0].pattern);
        assert_eq!(back.groups[0].xform, sc.groups[0].xform);
        assert_eq!((back.groups[0].uid, back.groups[0].instance_of), (sc.groups[0].uid, Some(9)));
        for p in [
            Pattern::Mirror { axes: [true, false, true], centre: true },
            Pattern::Rect { counts: [2, 3, 4], spacing: [1.0, 2.0, 3.0] },
            Pattern::Path { source: 4, count: 7, mode: PathMode::Step, step: 3.0, align: true },
            Pattern::Geometry { source: Source::Group(3), orient: true },
        ] {
            let t = serde_json::to_string(&p).unwrap_or_default();
            assert_eq!(serde_json::from_str::<Pattern>(&t).ok(), Some(p));
        }
        // a group with nothing new in it writes nothing new (old readers and old files agree)
        let plain = Scene::default().to_json();
        assert!(!plain.contains("pattern") && !plain.contains("instance_of") && !plain.contains("xform") && !plain.contains("uid"), "{plain}");
        // an old file opens as before; an unknown pattern is dropped, not fatal
        let old = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"shapes":[]}]}"#;
        let o = Scene::from_json(old).unwrap_or_default();
        assert_eq!(o.groups.len(), 1);
        assert!(o.groups[0].pattern.is_none() && o.groups[0].instance_of.is_none() && o.groups[0].xform.is_identity());
        let future = r#"{"groups":[{"name":"g","op":"Add","blend":0,"visible":true,"shapes":[],"pattern":{"Spiral":{"turns":3}}}]}"#;
        let f = Scene::from_json(future).unwrap_or_default();
        assert_eq!(f.groups.len(), 1);
        assert!(f.groups[0].pattern.is_none());
    }
}
