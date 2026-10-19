//! The maths of the pipe tool ("パイプ"): a freehand stroke becomes a clean list of curve nodes.
//!
//! 1. Every cursor position is projected into 3D (`trace_point`): onto the skin of the model under
//!    the cursor, lifted by the pipe radius so the pipe sits on it, or else onto a working plane.
//! 2. The raw samples are cleaned, lightly smoothed (a moving average that keeps both ends), and
//!    simplified with Ramer-Douglas-Peucker. The tolerance grows until the result fits in
//!    `PIPE_NODES` nodes.
//! 3. The nodes go into a `Kind::Curve` shape, whose own Catmull-Rom smoothing (`curve.rs`) turns
//!    them into the spline that is drawn and printed.
//!
//! Node budget: a curve keeps at most `model::MAX_PATH` (48) nodes; a traced pipe uses at most
//! `PIPE_NODES` (24) of them, which leaves room to add nodes by hand. The renderer measures at most
//! `curve::MAX_POINTS` (96) resampled points per curve (CPU, GPU and mesher alike), so 24 nodes still
//! get about four pieces per span. The GPU path list (`gpu::PATH_V4`) holds those for hundreds of curves.

use crate::model::MAX_PATH;
use crate::sdf::{Field, V3, add, dot, len, mul, sub};

/// Most nodes a traced pipe gets.
pub const PIPE_NODES: usize = 24;
/// Most raw samples one stroke keeps (a hostile or very long drag stops growing here).
pub const MAX_STROKE: usize = 2000;
/// The raw samples are thinned to about this many before simplifying (keeps RDP cheap).
const WORK_POINTS: usize = 400;
const COORD_LIMIT: f32 = 5000.0;
const _: () = assert!(PIPE_NODES <= MAX_PATH, "a traced pipe must fit in a curve");

// ---------------------------------------------------------------- 3D projection

/// The working plane for a stroke that misses the model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plane {
    /// The floor, z = 0.
    Floor,
    /// The front wall, y = 0.
    Front,
    /// The side wall, x = 0.
    Side,
}

impl Plane {
    pub const ALL: [Plane; 3] = [Plane::Floor, Plane::Front, Plane::Side];

    pub fn normal(self) -> V3 {
        match self {
            Plane::Floor => [0.0, 0.0, 1.0],
            Plane::Front => [0.0, 1.0, 0.0],
            Plane::Side => [1.0, 0.0, 0.0],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Plane::Floor => "Floor",
            Plane::Front => "Front",
            Plane::Side => "Side",
        }
    }
}

/// Where the ray `o + t d` meets the plane, lifted by `radius` toward the viewer so a pipe of that
/// radius lies on it. None when the ray is parallel to the plane, points away from it or is not finite.
pub fn plane_point(o: V3, d: V3, plane: Plane, radius: f32) -> Option<V3> {
    let n = plane.normal();
    let den = dot(d, n);
    if !(den.abs() > 1e-4) || !o.iter().chain(d.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let t = -dot(o, n) / den;
    if !(t.is_finite() && t > 0.0) {
        return None;
    }
    let hit = add(o, mul(d, t));
    // the viewer is on the side the ray comes from
    let side = if den < 0.0 { 1.0 } else { -1.0 };
    let lift = if radius.is_finite() { radius.clamp(0.0, 500.0) } else { 0.0 };
    let p = add(hit, mul(n, side * lift));
    p.iter().all(|v| v.is_finite() && v.abs() <= COORD_LIMIT).then_some(p)
}

/// The outward direction of the model's surface at `p` (the gradient of the distance field).
pub fn surface_normal(field: &Field, p: V3) -> Option<V3> {
    let e = 0.1;
    let g = [
        field.distance(add(p, [e, 0.0, 0.0])) - field.distance(sub(p, [e, 0.0, 0.0])),
        field.distance(add(p, [0.0, e, 0.0])) - field.distance(sub(p, [0.0, e, 0.0])),
        field.distance(add(p, [0.0, 0.0, e])) - field.distance(sub(p, [0.0, 0.0, e])),
    ];
    let l = len(g);
    (l.is_finite() && l > 1e-6).then(|| mul(g, 1.0 / l))
}

/// Where the ray meets the skin of the model, moved out along the surface normal by `radius` so a
/// pipe of that radius touches it. A couple of correction steps keep it at that distance on curved skin.
pub fn surface_point(field: &Field, o: V3, d: V3, radius: f32) -> Option<V3> {
    if !o.iter().chain(d.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let (t, _) = field.ray(o, d, 5000.0)?;
    let hit = add(o, mul(d, t));
    let r = if radius.is_finite() { radius.clamp(0.0, 500.0) } else { 0.0 };
    let n = surface_normal(field, hit).unwrap_or_else(|| mul(d, -1.0));
    let mut c = add(hit, mul(n, r));
    for _ in 0..2 {
        let gap = field.distance(c);
        if !gap.is_finite() {
            break;
        }
        // too close (negative) pushes out, too far pulls in; never jump more than the radius
        let fix = (r - gap).clamp(-r, r);
        c = add(c, mul(n, fix));
    }
    c.iter().all(|v| v.is_finite() && v.abs() <= COORD_LIMIT).then_some(c)
}

/// Where a cursor ray puts the next stroke sample: on the model's skin when it hits, else on the plane.
pub fn trace_point(field: &Field, o: V3, d: V3, plane: Plane, radius: f32) -> Option<V3> {
    surface_point(field, o, d, radius).or_else(|| plane_point(o, d, plane, radius))
}

/// Keep a short miss between two surface hits in 3D instead of letting its samples dive to the
/// working plane. A long miss stays on the plane so freehand drawing still behaves as expected.
pub fn bridge_surface_gaps(points: &mut [V3], on_skin: &[bool], plane_points: &[V3], radius: f32) {
    if points.len() != on_skin.len() || points.len() != plane_points.len() {
        return;
    }
    let max_span = (radius.max(0.3) * 24.0).clamp(12.0, 500.0);
    let mut start = 0;
    while start < points.len() {
        if !on_skin[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < points.len() && !on_skin[end] {
            end += 1;
        }
        if end < points.len() && end > start + 1 {
            let span = len(sub(points[end], points[start]));
            let plane_span = len(sub(plane_points[end], plane_points[start]));
            if span.is_finite() && plane_span.is_finite() && span <= max_span && plane_span <= max_span {
                let from = points[start];
                let to = points[end];
                for (offset, point) in points[start + 1..end].iter_mut().enumerate() {
                    let t = (offset + 1) as f32 / (end - start) as f32;
                    *point = add(from, mul(sub(to, from), t));
                }
            }
        }
        start = end;
    }
}

// ---------------------------------------------------------------- cleaning and smoothing

/// Finite points inside the working box, with repeats (closer than `eps`) dropped.
pub fn clean(points: &[V3], eps: f32) -> Vec<V3> {
    let mut out: Vec<V3> = Vec::with_capacity(points.len().min(MAX_STROKE));
    for p in points.iter().take(MAX_STROKE * 4) {
        if !p.iter().all(|v| v.is_finite()) {
            continue;
        }
        let p = [p[0].clamp(-COORD_LIMIT, COORD_LIMIT), p[1].clamp(-COORD_LIMIT, COORD_LIMIT), p[2].clamp(-COORD_LIMIT, COORD_LIMIT)];
        if out.last().is_none_or(|q| len(sub(p, *q)) > eps) {
            out.push(p);
        }
        if out.len() >= MAX_STROKE {
            break;
        }
    }
    out
}

/// Total length of the polyline.
pub fn path_length(points: &[V3]) -> f32 {
    points.windows(2).map(|w| len(sub(w[1], w[0]))).sum()
}

/// Keep about `n` of the points by an even spread of indices, both ends included.
pub fn thin(points: &[V3], n: usize) -> Vec<V3> {
    let n = n.max(2);
    if points.len() <= n {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..n).filter_map(|k| points.get(k * last / (n - 1))).copied().collect()
}

/// A moving average over three neighbours, `passes` times; both ends stay where they are. It takes
/// the shake out of a hand-drawn line without moving the line noticeably.
pub fn smooth(points: &[V3], passes: usize) -> Vec<V3> {
    let mut cur = points.to_vec();
    for _ in 0..passes.min(8) {
        if cur.len() < 3 {
            break;
        }
        let prev = cur.clone();
        for i in 1..prev.len() - 1 {
            if let (Some(a), Some(b), Some(c)) = (prev.get(i - 1), prev.get(i), prev.get(i + 1)) {
                cur[i] = [(a[0] + 2.0 * b[0] + c[0]) * 0.25, (a[1] + 2.0 * b[1] + c[1]) * 0.25, (a[2] + 2.0 * b[2] + c[2]) * 0.25];
            }
        }
    }
    cur
}

/// Distance from `p` to the segment `a`-`b`.
pub fn seg_dist(p: V3, a: V3, b: V3) -> f32 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 1e-12 { (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    len(sub(p, add(a, mul(ab, t))))
}

/// Ramer-Douglas-Peucker: the points that matter, within `tol` of the original line. The ends are
/// always kept. (Iterative, so a long stroke cannot overflow the stack.)
pub fn rdp(points: &[V3], tol: f32) -> Vec<V3> {
    let n = points.len();
    if n <= 2 {
        return points.to_vec();
    }
    let tol = if tol.is_finite() { tol.max(0.0) } else { 0.0 };
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (Some(pa), Some(pb)) = (points.get(a), points.get(b)) else { continue };
        let mut far = (0.0_f32, a);
        for i in a + 1..b {
            if let Some(p) = points.get(i) {
                let d = seg_dist(*p, *pa, *pb);
                if d > far.0 {
                    far = (d, i);
                }
            }
        }
        if far.0 > tol {
            if let Some(k) = keep.get_mut(far.1) {
                *k = true;
            }
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    points.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// RDP with a tolerance that grows until at most `max_nodes` points are left.
pub fn simplify(points: &[V3], tol: f32, max_nodes: usize) -> Vec<V3> {
    let max_nodes = max_nodes.max(2);
    let mut tol = if tol.is_finite() { tol.max(1e-3) } else { 1e-3 };
    for _ in 0..60 {
        let out = rdp(points, tol);
        if out.len() <= max_nodes {
            return out;
        }
        tol *= 1.3;
    }
    thin(points, max_nodes)
}

/// What to make of a stroke.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Largest distance the nodes may stray from the drawn line (mm), before the budget raises it.
    pub tol: f32,
    pub max_nodes: usize,
    /// The pipe is a closed loop: the last node is not a repeat of the first.
    pub closed: bool,
    /// A stroke shorter than this (mm) is a click, not a pipe.
    pub min_len: f32,
}

/// The nodes of a pipe for a raw stroke (world points), or None when the stroke is not a line:
/// fewer than two distinct points, or shorter than `min_len`. The first and last points of an open
/// stroke are the first and last nodes. A closed stroke is split at its farthest point so both
/// halves keep their corners; its last node is never a repeat of the first.
pub fn pipe_nodes(raw: &[V3], o: &Options) -> Option<Vec<V3>> {
    let mut pts = clean(raw, 1e-3);
    if pts.len() < 2 || !(path_length(&pts) >= o.min_len.max(1e-3)) {
        return None;
    }
    let max_nodes = o.max_nodes.clamp(2, MAX_PATH);
    pts = thin(&pts, WORK_POINTS);
    pts = smooth(&pts, 2);
    let tol = if o.tol.is_finite() { o.tol.max(1e-3) } else { 1.0 };
    let nodes = if o.closed && pts.len() >= 4 { closed_nodes(&pts, tol, max_nodes) } else { simplify(&pts, tol, max_nodes) };
    (nodes.len() >= 2).then_some(nodes)
}

fn closed_nodes(pts: &[V3], tol: f32, max_nodes: usize) -> Vec<V3> {
    let mut pts = pts.to_vec();
    // a loop drawn by hand ends near where it began: that last point is the first node again
    while pts.len() > 3 && pts.last().zip(pts.first()).is_some_and(|(l, f)| len(sub(*l, *f)) <= tol * 2.0) {
        pts.pop();
    }
    let Some(first) = pts.first().copied() else { return pts };
    let far = pts.iter().enumerate().max_by(|a, b| len(sub(*a.1, first)).total_cmp(&len(sub(*b.1, first)))).map(|(i, _)| i).unwrap_or(0);
    if far == 0 || far + 1 >= pts.len() {
        return simplify(&pts, tol, max_nodes);
    }
    let mut tol = tol;
    for _ in 0..60 {
        let a = rdp(&pts[..=far], tol);
        let mut tail = pts[far..].to_vec();
        tail.push(first);
        let b = rdp(&tail, tol);
        // a: first .. far;  b: far .. first (the closing repeat of the first is left off)
        let mut out = a;
        out.extend(b.iter().skip(1).take(b.len().saturating_sub(2)));
        if out.len() <= max_nodes {
            return out;
        }
        tol *= 1.3;
    }
    thin(&pts, max_nodes)
}

/// The middle of the box around the points.
pub fn centre(points: &[V3]) -> V3 {
    if points.is_empty() {
        return [0.0; 3];
    }
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Kind, Material, Scene, Shape};
    use crate::sdf::norm;

    fn opts() -> Options {
        Options { tol: 0.5, max_nodes: PIPE_NODES, closed: false, min_len: 2.0 }
    }

    fn sphere_field(r: f32) -> Field {
        let mut s = Shape::new(Kind::Sphere, [0.0; 3], Material::default());
        s.size[0] = r;
        s.blend = 0.0;
        let mut g = Group::new("g");
        g.shapes = vec![s];
        Field::new(&Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new(), anim: Default::default() })
    }

    /// A sine wave drawn with many tiny steps.
    fn wave(n: usize) -> Vec<V3> {
        (0..n)
            .map(|i| {
                let x = i as f32 * 100.0 / (n - 1) as f32;
                [x, 10.0 * (x * 0.1).sin(), 0.0]
            })
            .collect()
    }

    #[test]
    fn a_ray_on_the_skin_gives_a_point_one_radius_above_it() {
        let f = sphere_field(20.0);
        for (o, d) in [([100.0, 0.0, 0.0], [-1.0, 0.0, 0.0]), ([60.0, 60.0, 60.0], norm([-1.0, -1.0, -1.0])), ([5.0, 3.0, 90.0], [0.0, 0.0, -1.0])] {
            let p = surface_point(&f, o, d, 4.0).expect("hits the sphere");
            assert!((len(p) - 24.0).abs() < 0.15, "centre of a 4 mm pipe sits at 24 from the middle: {}", len(p));
        }
        // off the sphere: no skin hit
        assert!(surface_point(&f, [100.0, 50.0, 0.0], [-1.0, 0.0, 0.0], 4.0).is_none());
        // the normal of a sphere points away from its middle
        let n = surface_normal(&f, [20.0, 0.0, 0.0]).expect("normal");
        assert!(n[0] > 0.99);
    }

    #[test]
    fn a_ray_that_misses_the_model_lands_on_the_plane() {
        let o = [30.0, -40.0, 50.0];
        let d = norm([-0.2, 0.5, -1.0]);
        let p = plane_point(o, d, Plane::Floor, 4.0).expect("floor");
        assert!((p[2] - 4.0).abs() < 1e-4, "lifted by the radius toward the viewer: {p:?}");
        // seen from below the lift goes down
        let q = plane_point([0.0, 0.0, -30.0], [0.0, 0.0, 1.0], Plane::Floor, 4.0).expect("floor from below");
        assert!((q[2] + 4.0).abs() < 1e-4);
        // the other planes
        let front = plane_point([10.0, -50.0, 10.0], [0.0, 1.0, 0.0], Plane::Front, 3.0).expect("front");
        assert!((front[1] + 3.0).abs() < 1e-4 && front[0] == 10.0 && front[2] == 10.0);
        let side = plane_point([-50.0, 5.0, 7.0], [1.0, 0.0, 0.0], Plane::Side, 3.0).expect("side");
        assert!((side[0] + 3.0).abs() < 1e-4);
        // parallel, pointing away, hostile: nothing
        assert!(plane_point([0.0, 0.0, 10.0], [1.0, 0.0, 0.0], Plane::Floor, 4.0).is_none());
        assert!(plane_point([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], Plane::Floor, 4.0).is_none());
        assert!(plane_point([f32::NAN, 0.0, 10.0], [0.0, 0.0, -1.0], Plane::Floor, 4.0).is_none());
        // the combined rule: skin first, plane second
        let f = sphere_field(20.0);
        let on_skin = trace_point(&f, [100.0, 0.0, 0.0], [-1.0, 0.0, 0.0], Plane::Floor, 4.0).expect("skin");
        assert!((len(on_skin) - 24.0).abs() < 0.15);
        let on_floor = trace_point(&f, [100.0, 50.0, 100.0], norm([-1.0, 0.0, -1.0]), Plane::Floor, 4.0).expect("floor");
        assert!((on_floor[2] - 4.0).abs() < 1e-3);
    }

    #[test]
    fn a_stroke_between_two_spheres_bridges_the_empty_space() {
        let mut left = Shape::new(Kind::Sphere, [-20.0, 0.0, 20.0], Material::default());
        left.size[0] = 10.0;
        let mut right = Shape::new(Kind::Sphere, [20.0, 0.0, 20.0], Material::default());
        right.size[0] = 10.0;
        let mut g = Group::new("two balls");
        g.shapes = vec![left, right];
        let field = Field::new(&Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new(), anim: Default::default() });
        let mut points = Vec::new();
        let mut on_skin = Vec::new();
        let mut floor = Vec::new();
        for x in (-20..=20).step_by(2) {
            let o = [x as f32, 0.0, 100.0];
            let d = [0.0, 0.0, -1.0];
            let hit = surface_point(&field, o, d, 4.0);
            let fallback = plane_point(o, d, Plane::Floor, 4.0).expect("floor");
            points.push(hit.unwrap_or(fallback));
            on_skin.push(hit.is_some());
            floor.push(fallback);
        }
        assert!(points[10][2] < 5.0, "the old per-ray fallback drops to the floor");
        bridge_surface_gaps(&mut points, &on_skin, &floor, 4.0);
        assert!(on_skin.iter().any(|v| *v) && on_skin.iter().rev().any(|v| *v));
        assert!(points[10][2] > 20.0, "the midpoint should connect the balls in space: {:?}", points[10]);
        assert!(points[8..=12].iter().all(|p| p[2] > 20.0), "the whole gap stays in space");
    }

    #[test]
    fn a_floor_only_stroke_and_an_overlong_gap_keep_their_plane_points() {
        let floor: Vec<V3> = (0..12).map(|i| [i as f32 * 10.0, 0.0, 4.0]).collect();
        let mut only_floor = floor.clone();
        bridge_surface_gaps(&mut only_floor, &[false; 12], &floor, 4.0);
        assert_eq!(only_floor, floor);

        let mut too_far = floor.clone();
        too_far[0] = [0.0, 0.0, 34.0];
        too_far[11] = [600.0, 0.0, 34.0];
        let mut hits = [false; 12];
        hits[0] = true;
        hits[11] = true;
        bridge_surface_gaps(&mut too_far, &hits, &floor, 4.0);
        assert_eq!(&too_far[1..11], &floor[1..11], "a far jump must not form a giant bridge");
    }

    #[test]
    fn a_click_or_identical_points_make_nothing() {
        assert!(pipe_nodes(&[], &opts()).is_none());
        assert!(pipe_nodes(&[[1.0, 2.0, 3.0]], &opts()).is_none());
        assert!(pipe_nodes(&[[1.0, 2.0, 3.0]; 50], &opts()).is_none(), "a click: all the same point");
        // a tiny wiggle is shorter than min_len
        assert!(pipe_nodes(&[[0.0; 3], [0.4, 0.0, 0.0], [0.5, 0.2, 0.0]], &opts()).is_none());
        // hostile numbers are dropped, not trusted
        let bad = [[f32::NAN, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0], [0.0; 3]];
        assert!(pipe_nodes(&bad, &opts()).is_none());
        assert!(pipe_nodes(&[[0.0; 3], [1e30, 0.0, 0.0]], &opts()).is_some_and(|n| n.iter().all(|p| p.iter().all(|v| v.abs() <= COORD_LIMIT))));
    }

    #[test]
    fn a_straight_drag_is_two_nodes_and_keeps_its_ends() {
        let line: Vec<V3> = (0..80).map(|i| [i as f32 * 0.5, i as f32 * 0.25, 3.0]).collect();
        let n = pipe_nodes(&line, &opts()).expect("a line");
        assert_eq!(n.len(), 2);
        assert_eq!(n[0], line[0]);
        assert_eq!(n[1], line[79]);
    }

    #[test]
    fn a_wavy_stroke_is_thinned_within_the_budget_and_stays_close() {
        let raw = wave(900);
        let n = pipe_nodes(&raw, &opts()).expect("a wave");
        assert!(n.len() > 4 && n.len() <= PIPE_NODES, "{} nodes", n.len());
        assert_eq!(n[0], raw[0], "the start stays");
        assert_eq!(n[n.len() - 1], raw[899], "and so does the end");
        // every node lies near the drawn line
        for p in &n {
            let near = raw.windows(2).map(|w| seg_dist(*p, w[0], w[1])).fold(f32::MAX, f32::min);
            assert!(near < 0.6, "node {p:?} is {near} from the line");
        }
        // the smooth curve through the nodes follows the wave, not just the nodes
        let path: Vec<[f32; 4]> = n.iter().map(|p| [p[0], p[1], p[2], 3.0]).collect();
        let curve = crate::curve::resample(&path, 1.0, false);
        for q in curve.iter().step_by(3) {
            let near = raw.windows(2).map(|w| seg_dist([q[0], q[1], q[2]], w[0], w[1])).fold(f32::MAX, f32::min);
            assert!(near < 1.5, "spline point {q:?} strays {near}");
        }
        // a tighter tolerance never gives more than the budget either
        for tol in [0.001, 0.05, 5.0, 500.0, f32::NAN] {
            let n = pipe_nodes(&raw, &Options { tol, ..opts() }).expect("still a pipe");
            assert!((2..=PIPE_NODES).contains(&n.len()), "tol {tol}: {}", n.len());
        }
        // the node budget is respected even when asked for more than a curve can hold
        let n = pipe_nodes(&raw, &Options { tol: 0.001, max_nodes: 1000, ..opts() }).expect("pipe");
        assert!(n.len() <= MAX_PATH);
    }

    #[test]
    fn a_noisy_hand_gets_calmer() {
        // a straight line with jitter becomes (nearly) two nodes
        let mut seed = 5_u32;
        let mut r = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
        };
        let raw: Vec<V3> = (0..300).map(|i| [i as f32, r() * 0.8, r() * 0.8]).collect();
        let n = pipe_nodes(&raw, &Options { tol: 1.0, ..opts() }).expect("pipe");
        assert!(n.len() <= 3, "{} nodes for a jittery straight line", n.len());
    }

    #[test]
    fn a_closed_stroke_becomes_a_loop_without_a_repeated_node() {
        // a circle of radius 30 drawn once round, ending where it began
        let raw: Vec<V3> = (0..=200)
            .map(|i| {
                let a = i as f32 / 200.0 * std::f32::consts::TAU;
                [30.0 * a.cos(), 30.0 * a.sin(), 5.0]
            })
            .collect();
        let o = Options { closed: true, ..opts() };
        let n = pipe_nodes(&raw, &o).expect("a loop");
        assert!((4..=PIPE_NODES).contains(&n.len()), "{}", n.len());
        assert!(len(sub(n[0], n[n.len() - 1])) > 2.0, "the last node is not the first again");
        assert_eq!(n[0], raw[0]);
        for p in &n {
            assert!((len([p[0], p[1], 0.0]) - 30.0).abs() < 1.5, "{p:?} is off the circle");
        }
        // the loop as the curve sees it comes back to its first point
        let path: Vec<[f32; 4]> = n.iter().map(|p| [p[0], p[1], p[2], 3.0]).collect();
        let ring = crate::curve::resample(&path, 1.0, true);
        assert_eq!(ring.first(), ring.last());
        // a closed request on a degenerate (three point) stroke does not panic or make a loop of one
        let tiny = [[0.0; 3], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        assert!(pipe_nodes(&tiny, &o).is_some_and(|n| n.len() >= 2));
    }

    #[test]
    fn the_work_helpers_are_safe() {
        assert!(rdp(&[], 1.0).is_empty());
        assert_eq!(rdp(&[[0.0; 3]], 1.0).len(), 1);
        assert_eq!(thin(&[[0.0; 3]; 3], 0).len(), 2);
        assert_eq!(smooth(&[[0.0; 3], [1.0, 0.0, 0.0]], 5).len(), 2);
        assert!((seg_dist([0.0, 5.0, 0.0], [0.0; 3], [0.0; 3]) - 5.0).abs() < 1e-5, "a zero-length segment is a point");
        assert_eq!(centre(&[[0.0, 0.0, 0.0], [10.0, 4.0, -2.0]]), [5.0, 2.0, -1.0]);
        // a huge stroke is cut off at the cap
        let many: Vec<V3> = (0..50_000).map(|i| [i as f32, 0.0, 0.0]).collect();
        assert!(clean(&many, 1e-3).len() <= MAX_STROKE);
        assert!(pipe_nodes(&many, &opts()).is_some());
    }
}
