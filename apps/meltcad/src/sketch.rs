//! The geometry of the sketch workflow, with no window and no mouse: a drawing plane, the profiles
//! (rectangle, circle, polygon, spline), the checks (simple outline, winding) and the conversion of
//! a finished sketch into the numbers of a `Kind::Extrude / Revolve / Loft / Sweep` shape.
//! `sketch_ui.rs` is the glue to the viewport. All lengths are millimetres.

use crate::model::{Kind, MAX_OUTLINE, MAX_PATH, Shape};
use crate::sdf::{V3, add, cross, dot, len, mul, norm, rotation, sub};

/// Sketch coordinates stay inside this box (the same limit the file loader enforces on outlines).
pub const COORD_MAX: f32 = 2000.0;
/// Points on a circle.
pub const CIRCLE_POINTS: usize = 32;
/// The shortest side of a rectangle or radius of a circle that counts as a shape.
pub const MIN_SIDE: f32 = 0.5;
/// The smallest area a profile may have (mm²).
pub const MIN_AREA: f32 = 0.5;
/// Maximum number of independent through-holes in one sketch extrusion.
pub const MAX_HOLES: usize = 12;
/// Reject near-contact contours rather than generating zero-thickness walls (mm).
const MIN_CONTOUR_GAP: f32 = 0.05;
/// Samples per spline segment, at most.
const MAX_PER_SEGMENT: usize = 16;

// ------------------------------------------------------------------ errors

/// Why a sketch cannot be made into a shape. `message()` is what the person reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchError {
    TooFewPoints,
    BadNumber,
    TooManyPoints,
    TooSmall,
    SelfIntersect,
    HoleOutside,
    HoleOverlap,
    ContoursTouch,
    TooManyHoles,
    HoleMisaligned,
    /// A revolve profile reaches to the left of the axis.
    LeftOfAxis,
    /// A revolve needs a standing (vertical) plane.
    NeedsVerticalPlane,
    /// A sweep needs a path of at least two different points.
    PathTooShort,
}

impl SketchError {
    pub fn message(self) -> String {
        match self {
            SketchError::TooFewPoints => "Needs at least 3 points".to_string(),
            SketchError::BadNumber => "Invalid number. Please redraw".to_string(),
            SketchError::TooManyPoints => format!("Too many points (up to {MAX_OUTLINE})"),
            SketchError::TooSmall => "Too small (almost a line). Draw a larger shape".to_string(),
            SketchError::SelfIntersect => "Lines cross. Redraw without crossings".to_string(),
            SketchError::HoleOutside => "Hole outlines must be inside the outer outline".to_string(),
            SketchError::HoleOverlap => "Holes overlap. Separate them (holes cannot contain holes)".to_string(),
            SketchError::ContoursTouch => "Outlines cross or touch. Keep the outer edge and holes at least 0.05 mm apart".to_string(),
            SketchError::TooManyHoles => format!("Up to {MAX_HOLES} holes"),
            SketchError::HoleMisaligned => {
                "Hole and outer outline don't match in height or orientation. Use the same plane and extrude height".to_string()
            }
            SketchError::LeftOfAxis => "Extends past the center axis on the left. Draw on the right side".to_string(),
            SketchError::NeedsVerticalPlane => "Revolve needs a sketch on the Front or Side plane".to_string(),
            SketchError::PathTooShort => "Path too short. Place at least 2 points, spaced apart".to_string(),
        }
    }
}

// ------------------------------------------------------------------ the drawing plane

/// A plane in the world with a 2D coordinate system on it: the point (x, y) is
/// `origin + u * x + v * y`. `u` and `v` are at right angles; they are unit length for a sketch
/// plane, and may be longer (a scaled shape) when the frame is used to edit an outline.
/// The normal is `u x v`, so a profile drawn counter-clockwise faces along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: V3,
    pub u: V3,
    pub v: V3,
}

impl Frame {
    /// The floor (XY), seen from above: x to the right, y up the screen.
    pub fn floor() -> Frame {
        Frame::horizontal(0.0)
    }
    /// A level plane at height `z`.
    pub fn horizontal(z: f32) -> Frame {
        let z = if z.is_finite() { z.clamp(-5000.0, 5000.0) } else { 0.0 };
        Frame { origin: [0.0, 0.0, z], u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0] }
    }
    /// The XZ plane seen from the front (from -Y): x to the right, z up; it faces the viewer (-Y).
    pub fn front() -> Frame {
        Frame { origin: [0.0; 3], u: [1.0, 0.0, 0.0], v: [0.0, 0.0, 1.0] }
    }
    /// The YZ plane seen from the right side (from +X): y to the right, z up; it faces +X.
    pub fn side() -> Frame {
        Frame { origin: [0.0; 3], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0] }
    }
    /// Build a stable, right-handed drawing frame from two directions. The second
    /// axis is orthogonalized against the first; invalid/parallel axes are rejected
    /// instead of producing a broken sketch plane.
    pub fn from_axes(origin: V3, u: V3, v: V3) -> Option<Frame> {
        if !origin.iter().chain(u.iter()).chain(v.iter()).all(|x| x.is_finite() && x.abs() <= 1.0e6) {
            return None;
        }
        let lu = len(u);
        if lu < 1e-6 {
            return None;
        }
        let u = mul(u, 1.0 / lu);
        let v = sub(v, mul(u, dot(u, v)));
        let lv = len(v);
        if !lv.is_finite() || lv < 1e-6 {
            return None;
        }
        Some(Frame { origin, u, v: mul(v, 1.0 / lv) })
    }
    /// The plane that touches a surface at `point` whose outward direction is `normal`. The
    /// in-plane axes follow the nearest world axes: on a level face `u` is X (or Y when that fits
    /// better); on a standing face `u` is the level axis most along it and `v` points up. Normal +Z
    /// gives the floor's axes, +X the side's. A normal or point that is not finite (or a normal of
    /// no length) falls back to the floor.
    pub fn from_hit(point: V3, normal: V3) -> Frame {
        Self::try_from_hit(point, normal).unwrap_or_else(Frame::floor)
    }

    fn try_from_hit(point: V3, normal: V3) -> Option<Frame> {
        if !point.iter().chain(normal.iter()).all(|x| x.is_finite() && x.abs() <= 1.0e6) {
            return None;
        }
        let l = len(normal);
        if l < 1e-6 {
            return None;
        }
        let n = mul(normal, 1.0 / l);
        let level = n[2].abs() >= 0.7;
        let axes: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut best: Option<(f32, V3)> = None;
        // (only the level axes lead: a face is never turned about the vertical)
        for a in axes.iter().take(2) {
            let pr = sub(*a, mul(n, dot(*a, n)));
            let pl = len(pr);
            if pl.is_finite() && best.is_none_or(|b| pl > b.0 + 1e-6) {
                best = Some((pl, pr));
            }
        }
        let (pl, pr) = best?;
        if pl < 1e-6 {
            return None;
        }
        let mut u = mul(pr, 1.0 / pl);
        let mut v = cross(n, u);
        if !level && v[2] < 0.0 {
            u = mul(u, -1.0);
            v = mul(v, -1.0);
        }
        let origin = [point[0], point[1], point[2]];
        Frame::from_axes(origin, u, v)
    }

    /// Place a plane through a world point with X/Y/Z Euler rotation (degrees).
    /// The same rotation convention is used by saved sketch-generated shapes.
    pub fn oriented(origin: V3, degrees: V3) -> Option<Frame> {
        if !degrees.iter().all(|a| a.is_finite() && a.abs() <= 36000.0) {
            return None;
        }
        let r = rotation(degrees);
        Self::from_axes(origin, [r[0][0], r[1][0], r[2][0]], [r[0][1], r[1][1], r[2][1]])
    }
    /// The unit normal (`u x v`).
    pub fn normal(&self) -> V3 {
        norm(cross(self.u, self.v))
    }
    /// A standing plane (the normal is level)?
    pub fn is_vertical(&self) -> bool {
        self.normal()[2].abs() < 0.02
    }
    pub fn to_world(self, p: [f32; 2]) -> V3 {
        add(self.origin, add(mul(self.u, p[0]), mul(self.v, p[1])))
    }
    /// The sketch coordinates of a world point (its distance from the plane is dropped).
    #[allow(clippy::wrong_self_convention)] // Paired forward/inverse frame transforms share one receiver.
    pub fn from_world(&self, w: V3) -> [f32; 2] {
        let d = sub(w, self.origin);
        [dot(d, self.u) / dot(self.u, self.u).max(1e-12), dot(d, self.v) / dot(self.v, self.v).max(1e-12)]
    }
    /// Where a ray (origin `o`, direction `d`) meets the plane, in sketch coordinates. None when the
    /// ray runs along the plane, the plane is behind it, or the hit is absurdly far.
    pub fn hit(&self, o: V3, d: V3) -> Option<[f32; 2]> {
        let n = self.normal();
        let den = dot(d, n);
        if den.abs() < 1e-3 {
            return None;
        }
        let t = dot(sub(self.origin, o), n) / den;
        if !t.is_finite() || t <= 0.0 {
            return None;
        }
        let p = self.from_world(add(o, mul(d, t)));
        (p[0].is_finite() && p[1].is_finite() && p[0].abs() <= COORD_MAX && p[1].abs() <= COORD_MAX).then_some(p)
    }
    /// The Euler angles (degrees, as `sdf::rotation` reads them) of a shape whose own x, y and z
    /// axes lie along this frame's `u`, `v` and normal.
    pub fn euler(&self) -> V3 {
        euler_from_columns(norm(self.u), norm(self.v), self.normal())
    }
}

/// The Euler angles (X, then Y, then Z; degrees) of the rotation whose columns are `u`, `v`, `n`.
pub fn euler_from_columns(u: V3, v: V3, n: V3) -> V3 {
    let r = [[u[0], v[0], n[0]], [u[1], v[1], n[1]], [u[2], v[2], n[2]]];
    let sy = (-r[2][0]).clamp(-1.0, 1.0);
    let (x, y, z) = if sy.abs() < 0.999_999 {
        (r[2][1].atan2(r[2][2]), sy.asin(), r[1][0].atan2(r[0][0]))
    } else {
        // looking straight along an axis: the turn about X and Z are the same turn
        ((-r[1][2]).atan2(r[1][1]), sy.asin(), 0.0)
    };
    let deg = |a: f32| if a.is_finite() { a.to_degrees() } else { 0.0 };
    [deg(x), deg(y), deg(z)]
}

/// How high the top of a shape is in the world (exact for shapes that are not turned, a safe upper
/// bound for turned ones).
pub fn top_of(s: &Shape) -> f32 {
    let h = s.local_half();
    let r = rotation(s.rot);
    let reach: f32 = (0..3).map(|i| r[2][i].abs() * h[i] * s.scale[i]).sum();
    let top = s.pos[2] + reach;
    if top.is_finite() { top } else { 0.0 }
}

// ------------------------------------------------------------------ snapping

/// Round `p` to a grid of `step` mm.
pub fn snap(p: [f32; 2], step: f32) -> [f32; 2] {
    let s = if step.is_finite() && step > 1e-4 { step } else { 1.0 };
    let r = |v: f32| (v / s).round() * s;
    // rounding a multiple of 0.1 leaves tiny float noise: tidy it
    let t = |v: f32| (r(v) * 1000.0).round() / 1000.0;
    [t(p[0]), t(p[1])]
}

// ------------------------------------------------------------------ screen-space snapping

/// Snap priorities (most intentional first). Distance is measured in SCREEN pixels, never in mm.
/// The weights are small enough that a much closer target wins over a distant high-priority one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Origin,
    Endpoint,
    Intersection,
    Center,
    Midpoint,
    Horizontal,
    Vertical,
    Angle,
    Grid,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Origin => "Origin",
            Self::Endpoint => "Endpoint",
            Self::Intersection => "Intersection",
            Self::Center => "Center",
            Self::Midpoint => "Midpoint",
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::Angle => "15°",
            Self::Grid => "Grid",
        }
    }

    fn penalty(self) -> f32 {
        match self {
            Self::Origin => 0.0,
            Self::Endpoint => 1.0,
            Self::Intersection => 2.0,
            Self::Center => 3.0,
            Self::Midpoint => 4.0,
            // alignment guides lose to a real point that is a few pixels away
            Self::Horizontal | Self::Vertical => 8.0,
            Self::Angle => 8.5,
            Self::Grid => 9.0,
        }
    }
}

/// Acquiring a target is deliberately narrower than keeping one already selected.
pub const SNAP_RADIUS_PX: f32 = 11.0;
pub const SNAP_HOLD_PX: f32 = 17.0;
/// A competing target must be noticeably closer before replacing the current target.
const SNAP_SWITCH_PX: f32 = 5.0;
const MAX_SNAP_POINTS: usize = 768;
const MAX_SNAP_EDGES: usize = 384;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapHit {
    pub at: [f32; 2],
    pub kind: SnapKind,
}

/// The current sketch and coplanar saved outlines. Omitted points cannot attract their own drag.
#[derive(Default, Debug)]
pub struct SnapGeometry {
    pub points: Vec<SnapHit>,
    pub edges: Vec<([f32; 2], [f32; 2])>,
}

impl SnapGeometry {
    pub fn add_outline(&mut self, pts: &[[f32; 2]], closed: bool, omit: Option<usize>) {
        let n = pts.len();
        if n == 0 || !pts.iter().all(finite) {
            return;
        }
        let edges = if closed && n > 2 { n } else { n.saturating_sub(1) };
        if n > MAX_SNAP_POINTS || self.points.len().saturating_add(n + 1) > MAX_SNAP_POINTS || self.edges.len().saturating_add(edges) > MAX_SNAP_EDGES
        {
            return;
        }
        for (i, &at) in pts.iter().enumerate() {
            if Some(i) != omit {
                self.points.push(SnapHit { at, kind: SnapKind::Endpoint });
            }
        }
        for i in 0..edges {
            let next = (i + 1) % n;
            if Some(i) == omit || Some(next) == omit || pts[i] == pts[next] {
                continue;
            }
            let (a, b) = (pts[i], pts[next]);
            self.edges.push((a, b));
            self.points.push(SnapHit { at: [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5], kind: SnapKind::Midpoint });
        }
        if closed
            && n > 2
            && omit.is_none()
            && let Some((lo, hi)) = bbox(pts)
        {
            self.points.push(SnapHit { at: [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5], kind: SnapKind::Center });
        }
    }
}

/// Only a proper intersection of two finite segments is an intersection snap.
/// Endpoint contacts have their own (higher-priority) snap type; overlaps are excluded.
pub fn snap_intersection(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<[f32; 2]> {
    if ![a, b, c, d].iter().all(finite) {
        return None;
    }
    let (rx, ry) = ((b[0] - a[0]) as f64, (b[1] - a[1]) as f64);
    let (sx, sy) = ((d[0] - c[0]) as f64, (d[1] - c[1]) as f64);
    let (qx, qy) = ((c[0] - a[0]) as f64, (c[1] - a[1]) as f64);
    let den = rx * sy - ry * sx;
    let scale = rx.hypot(ry) * sx.hypot(sy);
    if scale < 1e-12 || den.abs() <= scale * 1e-10 {
        return None;
    }
    let t = (qx * sy - qy * sx) / den;
    let u = (qx * ry - qy * rx) / den;
    if !(1e-6..=1.0 - 1e-6).contains(&t) || !(1e-6..=1.0 - 1e-6).contains(&u) {
        return None;
    }
    let p = [(a[0] as f64 + t * rx) as f32, (a[1] as f64 + t * ry) as f32];
    finite(&p).then_some(p)
}

/// Project the cursor onto the closest angle multiple, keeping the point on the ray
/// from the previous vertex. Used only when the optional angle mode is enabled.
pub fn snap_angle(anchor: [f32; 2], raw: [f32; 2], increment_deg: f32) -> Option<[f32; 2]> {
    if !finite(&anchor) || !finite(&raw) || !increment_deg.is_finite() || !(0.1..=90.0).contains(&increment_deg) {
        return None;
    }
    let (dx, dy) = (raw[0] - anchor[0], raw[1] - anchor[1]);
    if dx.hypot(dy) < 1e-3 {
        return None;
    }
    let step = increment_deg.to_radians();
    let angle = (dy.atan2(dx) / step).round() * step;
    let (x, y) = (angle.cos(), angle.sin());
    let distance = dx * x + dy * y;
    let q = [anchor[0] + distance * x, anchor[1] + distance * y];
    (distance > 0.0 && finite(&q)).then_some(q)
}

/// Pick with distance in logical screen pixels (egui points). A stable lock/hysteresis avoids
/// flickering between nearly coincident candidates when the mouse moves by one pixel.
pub fn choose_screen_snap(pointer: [f32; 2], candidates: &[(SnapHit, [f32; 2])], previous: Option<SnapHit>) -> Option<SnapHit> {
    if !pointer[0].is_finite() || !pointer[1].is_finite() {
        return None;
    }
    let distance = |px: [f32; 2]| (px[0] - pointer[0]).hypot(px[1] - pointer[1]);
    let best = candidates
        .iter()
        .filter_map(|(hit, px)| {
            let d = distance(*px);
            (d.is_finite() && d <= SNAP_RADIUS_PX).then_some((*hit, d))
        })
        .min_by(|(a, da), (b, db)| {
            (da + a.kind.penalty())
                .total_cmp(&(db + b.kind.penalty()))
                .then_with(|| a.kind.penalty().total_cmp(&b.kind.penalty()))
                .then_with(|| da.total_cmp(db))
        });
    let held = previous.and_then(|old| {
        candidates
            .iter()
            .filter(|(hit, _)| hit.kind == old.kind && dist(hit.at, old.at) < 1e-3)
            .map(|(hit, px)| (*hit, distance(*px)))
            .filter(|(_, d)| d.is_finite() && *d <= SNAP_HOLD_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    });
    match (held, best) {
        (Some((locked, old_d)), Some((new, new_d))) if locked != new => {
            // Priority influences initial selection, but may not make a held snap jump.
            let old_score = old_d + locked.kind.penalty();
            let new_score = new_d + new.kind.penalty();
            if new_d + SNAP_SWITCH_PX < old_d && new_score + 2.0 < old_score { Some(new) } else { Some(locked) }
        }
        (Some((locked, _)), _) => Some(locked),
        (None, best) => best.map(|(hit, _)| hit),
    }
}

// ------------------------------------------------------------------ small 2D helpers

fn finite(p: &[f32; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite() && p[0].abs() <= COORD_MAX && p[1].abs() <= COORD_MAX
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Twice the signed area is `sum(x_i * y_(i+1) - x_(i+1) * y_i)`; positive = counter-clockwise.
pub fn signed_area(pts: &[[f32; 2]]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0_f64;
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        a += f64::from(p[0]) * f64::from(q[1]) - f64::from(q[0]) * f64::from(p[1]);
    }
    (a * 0.5) as f32
}

/// Turn the outline counter-clockwise.
pub fn ensure_ccw(pts: &mut [[f32; 2]]) {
    if signed_area(pts) < 0.0 {
        pts.reverse();
    }
}

/// The corners of the box around the points (None for no points).
pub fn bbox(pts: &[[f32; 2]]) -> Option<([f32; 2], [f32; 2])> {
    let first = pts.first()?;
    let mut lo = *first;
    let mut hi = *first;
    for p in pts {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    Some((lo, hi))
}

/// The middle of the box around the points, rounded to a whole millimetre.
pub fn centre(pts: &[[f32; 2]]) -> [f32; 2] {
    match bbox(pts) {
        Some((lo, hi)) => snap([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5], 1.0),
        None => [0.0; 2],
    }
}

/// Drop repeated neighbours (and a last point equal to the first).
pub fn dedupe(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_none_or(|q| dist(*q, *p) > 1e-3) {
            out.push(*p);
        }
    }
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| dist(*a, *b) <= 1e-3) {
        out.pop();
    }
    out
}

/// Drop repeated neighbours only (an open line may end where it began).
pub fn dedupe_open(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last().is_none_or(|q| dist(*q, *p) > 1e-3) {
            out.push(*p);
        }
    }
    out
}

/// At most `cap` of the points, evenly spread, keeping the first and the last.
fn thin(pts: Vec<[f32; 2]>, cap: usize) -> Vec<[f32; 2]> {
    let n = pts.len();
    if n <= cap || cap < 2 {
        return pts;
    }
    (0..cap).map(|i| pts[i * (n - 1) / (cap - 1)]).collect()
}

fn orient(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn within(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    c[0] >= a[0].min(b[0]) - 1e-5 && c[0] <= a[0].max(b[0]) + 1e-5 && c[1] >= a[1].min(b[1]) - 1e-5 && c[1] <= a[1].max(b[1]) + 1e-5
}

/// Do the segments p1-p2 and p3-p4 cross or touch?
pub fn segments_meet(p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], p4: [f32; 2]) -> bool {
    const EPS: f32 = 1e-5;
    let d1 = orient(p3, p4, p1);
    let d2 = orient(p3, p4, p2);
    let d3 = orient(p1, p2, p3);
    let d4 = orient(p1, p2, p4);
    if ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS)) && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS)) {
        return true;
    }
    (d1.abs() <= EPS && within(p3, p4, p1))
        || (d2.abs() <= EPS && within(p3, p4, p2))
        || (d3.abs() <= EPS && within(p1, p2, p3))
        || (d4.abs() <= EPS && within(p1, p2, p4))
}

/// Is the closed outline free of crossings, touching edges and sharp fold-backs?
pub fn is_simple(pts: &[[f32; 2]]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        // two neighbouring edges that run back over each other
        let c = pts[(i + 2) % n];
        if orient(a, b, c).abs() <= 1e-5 * dist(a, b).max(1.0) * dist(b, c).max(1.0)
            && (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]) < 0.0
        {
            return false;
        }
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_meet(a, b, pts[j], pts[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}

/// Would a new segment `a`-`b` cross the open polyline `pts` (whose last point is `a`)? The segment
/// may not run back over the last segment. `closing` means `b` is the first point, which the new
/// segment is allowed to touch.
pub fn crosses_polyline(pts: &[[f32; 2]], a: [f32; 2], b: [f32; 2], closing: bool) -> bool {
    let n = pts.len();
    if n < 2 {
        return false;
    }
    // every segment before the one that ends at `a`
    for i in 0..n - 2 {
        let (p, q) = (pts[i], pts[i + 1]);
        if !segments_meet(a, b, p, q) {
            continue;
        }
        // closing: the first segment starts at `b`, so they meet there by design
        if closing && i == 0 && closing_touch_only(a, b, p, q) {
            continue;
        }
        return true;
    }
    // folding straight back over the last segment
    if let Some(prev) = pts.get(n - 2) {
        let o = orient(*prev, a, b);
        if o.abs() <= 1e-5 * dist(*prev, a).max(1.0) * dist(a, b).max(1.0)
            && (a[0] - prev[0]) * (b[0] - a[0]) + (a[1] - prev[1]) * (b[1] - a[1]) < 0.0
        {
            return true;
        }
    }
    false
}

/// Do segments a-b and p-q (where p == b) meet only at b?
fn closing_touch_only(a: [f32; 2], b: [f32; 2], p: [f32; 2], q: [f32; 2]) -> bool {
    if dist(p, b) > 1e-3 {
        return false;
    }
    // a-b and b-q share the end b: they only overlap if they point the same way along one line
    let o = orient(a, b, q);
    let same_way = (a[0] - b[0]) * (q[0] - b[0]) + (a[1] - b[1]) * (q[1] - b[1]) > 0.0;
    !(o.abs() <= 1e-5 * dist(a, b).max(1.0) * dist(b, q).max(1.0) && same_way)
}

// ------------------------------------------------------------------ profile generators

/// A rectangle with corners `a` and `b` (any two opposite corners), counter-clockwise.
pub fn rect(a: [f32; 2], b: [f32; 2]) -> Vec<[f32; 2]> {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// A circle of `n` points around `c`, counter-clockwise, starting at the right.
pub fn circle(c: [f32; 2], r: f32, n: usize) -> Vec<[f32; 2]> {
    let n = n.clamp(3, MAX_OUTLINE);
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// One point of the centripetal Catmull-Rom curve through `p1` and `p2` (`u` in 0..1).
fn catmull(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], u: f32) -> [f32; 2] {
    let knot = |a: [f32; 2], b: [f32; 2]| dist(a, b).max(1e-6).sqrt();
    let t0 = 0.0_f32;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + (t2 - t1) * u;
    let mix = |a: [f32; 2], b: [f32; 2], ta: f32, tb: f32| {
        let w = tb - ta;
        if w.abs() < 1e-9 {
            return a;
        }
        let (k, l) = ((tb - t) / w, (t - ta) / w);
        [a[0] * k + b[0] * l, a[1] * k + b[1] * l]
    };
    let a1 = mix(p0, p1, t0, t1);
    let a2 = mix(p1, p2, t1, t2);
    let a3 = mix(p2, p3, t2, t3);
    let b1 = mix(a1, a2, t0, t2);
    let b2 = mix(a2, a3, t1, t3);
    mix(b1, b2, t1, t2)
}

/// A smooth closed curve through the control points, as at most `max_points` points.
pub fn spline_closed(ctrl: &[[f32; 2]], max_points: usize) -> Vec<[f32; 2]> {
    let cap = max_points.clamp(3, MAX_OUTLINE);
    let c = thin(dedupe(&ctrl.iter().copied().filter(finite).collect::<Vec<_>>()), cap);
    let n = c.len();
    if n < 3 {
        return c;
    }
    let k = (cap / n).clamp(1, MAX_PER_SEGMENT);
    let mut out = Vec::with_capacity(n * k);
    for i in 0..n {
        let (p0, p1, p2, p3) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n], c[(i + 2) % n]);
        out.push(p1);
        for j in 1..k {
            out.push(catmull(p0, p1, p2, p3, j as f32 / k as f32));
        }
    }
    out
}

/// A smooth open curve through the control points (the first and the last stay as they are), as at
/// most `max_points` points.
pub fn spline_open(ctrl: &[[f32; 2]], max_points: usize) -> Vec<[f32; 2]> {
    let cap = max_points.clamp(3, MAX_PATH.max(MAX_OUTLINE));
    let c = thin(dedupe_open(&ctrl.iter().copied().filter(finite).collect::<Vec<_>>()), cap);
    let n = c.len();
    if n < 3 {
        return c;
    }
    let k = ((cap - 1) / (n - 1)).clamp(1, MAX_PER_SEGMENT);
    // mirror the end points so the curve leaves them naturally
    let first = [2.0 * c[0][0] - c[1][0], 2.0 * c[0][1] - c[1][1]];
    let last = [2.0 * c[n - 1][0] - c[n - 2][0], 2.0 * c[n - 1][1] - c[n - 2][1]];
    let at = |i: isize| -> [f32; 2] {
        if i < 0 {
            first
        } else if i as usize >= n {
            last
        } else {
            c[i as usize]
        }
    };
    let mut out = Vec::with_capacity((n - 1) * k + 1);
    for (i, point) in c.iter().enumerate().take(n - 1) {
        let ii = i as isize;
        out.push(*point);
        for j in 1..k {
            out.push(catmull(at(ii - 1), at(ii), at(ii + 1), at(ii + 2), j as f32 / k as f32));
        }
    }
    out.push(c[n - 1]);
    out
}

// ------------------------------------------------------------------ checking a profile

/// A finished closed outline: no repeats, counter-clockwise, simple, not tiny, at most
/// `MAX_OUTLINE` points. The error says what to fix.
pub fn validate_profile(pts: &[[f32; 2]]) -> Result<Vec<[f32; 2]>, SketchError> {
    if !pts.iter().all(finite) {
        return Err(SketchError::BadNumber);
    }
    let mut p = dedupe(pts);
    if p.len() < 3 {
        return Err(SketchError::TooFewPoints);
    }
    if p.len() > MAX_OUTLINE {
        return Err(SketchError::TooManyPoints);
    }
    if is_flat(&p) {
        return Err(SketchError::TooSmall);
    }
    if !is_simple(&p) {
        return Err(SketchError::SelfIntersect);
    }
    if signed_area(&p).abs() < MIN_AREA {
        return Err(SketchError::TooSmall);
    }
    ensure_ccw(&mut p);
    Ok(p)
}

/// Square of the distance from a point to a finite line segment.
fn point_segment_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len2 = dx * dx + dy * dy;
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2.max(1e-12)).clamp(0.0, 1.0);
    let x = p[0] - (a[0] + t * dx);
    let y = p[1] - (a[1] + t * dy);
    x * x + y * y
}

/// Do two closed loops cross, touch, or come dangerously close to one another?
fn contours_touch(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    let gap2 = MIN_CONTOUR_GAP * MIN_CONTOUR_GAP;
    for i in 0..a.len() {
        let (a0, a1) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            let (b0, b1) = (b[j], b[(j + 1) % b.len()]);
            if segments_meet(a0, a1, b0, b1)
                || point_segment_dist2(a0, b0, b1) < gap2
                || point_segment_dist2(a1, b0, b1) < gap2
                || point_segment_dist2(b0, a0, a1) < gap2
                || point_segment_dist2(b1, a0, a1) < gap2
            {
                return true;
            }
        }
    }
    false
}

/// Validate an outer profile plus zero or more independent holes. Every hole must be strictly
/// inside the outer loop; overlaps, nested holes and tangent edges are rejected. Unlike a
/// triangulator this only validates 2D topology: existing SDF Boolean subtraction makes the solid.
pub fn validate_holes(outer: &[[f32; 2]], holes: &[Vec<[f32; 2]>]) -> Result<Vec<Vec<[f32; 2]>>, SketchError> {
    let outer = validate_profile(outer)?;
    if holes.len() > MAX_HOLES {
        return Err(SketchError::TooManyHoles);
    }
    let mut accepted: Vec<Vec<[f32; 2]>> = Vec::with_capacity(holes.len());
    for hole in holes {
        let h = validate_profile(hole)?;
        if contours_touch(&outer, &h) {
            return Err(SketchError::ContoursTouch);
        }
        if crate::sdf::polygon(&outer, h[0]) >= -MIN_CONTOUR_GAP {
            return Err(SketchError::HoleOutside);
        }
        for prev in &accepted {
            if contours_touch(prev, &h) {
                return Err(SketchError::ContoursTouch);
            }
            if crate::sdf::polygon(prev, h[0]) < 0.0 || crate::sdf::polygon(&h, prev[0]) < 0.0 {
                return Err(SketchError::HoleOverlap);
            }
        }
        accepted.push(h);
    }
    Ok(accepted)
}

/// Do all the points lie (almost) on one straight line?
fn is_flat(pts: &[[f32; 2]]) -> bool {
    let Some(a) = pts.first().copied() else { return true };
    let b = pts.iter().copied().fold(a, |far, q| if dist(a, q) > dist(a, far) { q } else { far });
    let l = dist(a, b);
    if l < 1e-3 {
        return true;
    }
    pts.iter().all(|q| orient(a, b, *q).abs() / l < 0.05)
}

/// A rectangle from two corners, checked.
pub fn rect_profile(a: [f32; 2], b: [f32; 2]) -> Result<Vec<[f32; 2]>, SketchError> {
    if (a[0] - b[0]).abs() < MIN_SIDE || (a[1] - b[1]).abs() < MIN_SIDE {
        return Err(SketchError::TooSmall);
    }
    validate_profile(&rect(a, b))
}

/// A circle from its centre and a point on it, checked.
pub fn circle_profile(c: [f32; 2], edge: [f32; 2]) -> Result<Vec<[f32; 2]>, SketchError> {
    let r = dist(c, edge);
    if !r.is_finite() || r < MIN_SIDE {
        return Err(SketchError::TooSmall);
    }
    validate_profile(&circle(c, r, CIRCLE_POINTS))
}

/// A closed outline from the clicked points: corners as they are, or a smooth curve through them.
pub fn closed_profile(ctrl: &[[f32; 2]], smooth: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    if !ctrl.iter().all(finite) {
        return Err(SketchError::BadNumber);
    }
    let pts = if smooth { spline_closed(ctrl, MAX_OUTLINE) } else { dedupe(ctrl) };
    validate_profile(&pts)
}

/// The 2D points of an open line from the clicked points (corners, or a smooth curve), at most
/// `MAX_PATH`.
pub fn open_line(ctrl: &[[f32; 2]], smooth: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    if !ctrl.iter().all(finite) {
        return Err(SketchError::BadNumber);
    }
    let pts = if smooth { spline_open(ctrl, MAX_PATH) } else { thin(dedupe_open(ctrl), MAX_PATH) };
    let total: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    if pts.len() < 2 || total < 1.0 {
        return Err(SketchError::PathTooShort);
    }
    Ok(pts)
}

/// The same outline scaled about the middle of its box (a loft's top as a smaller or larger copy).
pub fn scaled(pts: &[[f32; 2]], s: f32) -> Vec<[f32; 2]> {
    let s = if s.is_finite() { s.clamp(0.05, 20.0) } else { 1.0 };
    let c = match bbox(pts) {
        Some((lo, hi)) => [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5],
        None => [0.0; 2],
    };
    pts.iter().map(|p| [c[0] + (p[0] - c[0]) * s, c[1] + (p[1] - c[1]) * s]).collect()
}

// ------------------------------------------------------------------ precise edits (keep the existing outline format)

// These edits operate on the polygon already stored in Shape::outline / outline2.
// There is deliberately no constraint graph or new serialized metadata.

/// Validate a proposed edit without silently removing or reordering any vertices.
/// Reversing the winding would reverse the solid's face normals, so reject it too.
pub fn validate_edit(before: &[[f32; 2]], after: &[[f32; 2]], revolve: bool) -> Result<(), SketchError> {
    if before.len() != after.len() || after.len() < 3 {
        return Err(SketchError::TooFewPoints);
    }
    if revolve && after.iter().any(|p| p[0] < 0.0) {
        return Err(SketchError::LeftOfAxis);
    }
    let checked = validate_profile(after)?;
    if checked.len() != after.len() {
        return Err(SketchError::TooSmall); // a duplicate endpoint would disappear
    }
    if signed_area(before) * signed_area(after) <= 0.0 {
        return Err(SketchError::TooSmall);
    }
    Ok(())
}

/// Move one vertex while its two neighbouring edges remain connected.
pub fn move_vertex(pts: &[[f32; 2]], i: usize, at: [f32; 2], revolve: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    if i >= pts.len() {
        return Err(SketchError::TooFewPoints);
    }
    let mut out = pts.to_vec();
    out[i] = at;
    validate_edit(pts, &out, revolve)?;
    Ok(out)
}

/// Change the length of edge i -> i+1, keeping the first endpoint fixed.
/// Other edges still share the moved endpoint, so the polygon cannot open.
pub fn set_edge_length(pts: &[[f32; 2]], i: usize, length: f32, revolve: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    if i >= pts.len() || pts.len() < 3 {
        return Err(SketchError::TooFewPoints);
    }
    if !length.is_finite() {
        return Err(SketchError::BadNumber);
    }
    if length < MIN_SIDE {
        return Err(SketchError::TooSmall);
    }
    let next = (i + 1) % pts.len();
    let old = dist(pts[i], pts[next]);
    if old <= 1e-3 {
        return Err(SketchError::TooSmall);
    }
    let factor = length / old;
    let at = [pts[i][0] + (pts[next][0] - pts[i][0]) * factor, pts[i][1] + (pts[next][1] - pts[i][1]) * factor];
    move_vertex(pts, next, at, revolve)
}

/// Recognise the exact axis-aligned rectangle form without changing user-drawn polygons.
pub fn rectangle_size(pts: &[[f32; 2]]) -> Option<[f32; 2]> {
    if pts.len() != 4 || validate_profile(pts).is_err() {
        return None;
    }
    let (lo, hi) = bbox(pts)?;
    let w = hi[0] - lo[0];
    let h = hi[1] - lo[1];
    if w < MIN_SIDE || h < MIN_SIDE {
        return None;
    }
    let corners = rect(lo, hi);
    if pts.iter().all(|p| corners.iter().any(|q| dist(*p, *q) < 1e-3)) { Some([w, h]) } else { None }
}

/// Resize a rectangle about its current centre (no translation of the solid).
pub fn set_rectangle_size(pts: &[[f32; 2]], width: f32, height: f32, revolve: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    let Some(_) = rectangle_size(pts) else { return Err(SketchError::SelfIntersect) };
    if !width.is_finite() || !height.is_finite() {
        return Err(SketchError::BadNumber);
    }
    if width < MIN_SIDE || height < MIN_SIDE {
        return Err(SketchError::TooSmall);
    }
    let (lo, hi) = bbox(pts).ok_or(SketchError::TooFewPoints)?;
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let out: Vec<_> = pts
        .iter()
        .map(|p| [c[0] + if p[0] < c[0] { -width * 0.5 } else { width * 0.5 }, c[1] + if p[1] < c[1] { -height * 0.5 } else { height * 0.5 }])
        .collect();
    validate_edit(pts, &out, revolve)?;
    Ok(out)
}

/// Diameter of a regular 32-segment circle. Once a vertex is freely moved,
/// it is no longer treated as a circle and generic point/edge editing applies.
pub fn circle_diameter(pts: &[[f32; 2]]) -> Option<f32> {
    if pts.len() != CIRCLE_POINTS {
        return None;
    }
    let n = pts.len() as f32;
    let center = [pts.iter().map(|p| p[0]).sum::<f32>() / n, pts.iter().map(|p| p[1]).sum::<f32>() / n];
    let r = pts.iter().map(|p| dist(*p, center)).sum::<f32>() / n;
    if !r.is_finite() || r < MIN_SIDE {
        return None;
    }
    let expected_chord = 2.0 * r * (std::f32::consts::PI / n).sin();
    if pts.iter().enumerate().any(|(i, p)| {
        (dist(*p, center) - r).abs() > r * 0.003 + 1e-4 || (dist(*p, pts[(i + 1) % pts.len()]) - expected_chord).abs() > r * 0.003 + 1e-4
    }) {
        return None;
    }
    Some(2.0 * r)
}

/// Scale a recognised circle about its centre, retaining its 32 vertices.
pub fn set_circle_diameter(pts: &[[f32; 2]], diameter: f32, revolve: bool) -> Result<Vec<[f32; 2]>, SketchError> {
    let Some(old) = circle_diameter(pts) else { return Err(SketchError::SelfIntersect) };
    if !diameter.is_finite() {
        return Err(SketchError::BadNumber);
    }
    if diameter < MIN_SIDE * 2.0 {
        return Err(SketchError::TooSmall);
    }
    let n = pts.len() as f32;
    let center = [pts.iter().map(|p| p[0]).sum::<f32>() / n, pts.iter().map(|p| p[1]).sum::<f32>() / n];
    let factor = diameter / old;
    let out: Vec<_> = pts.iter().map(|p| [center[0] + (p[0] - center[0]) * factor, center[1] + (p[1] - center[1]) * factor]).collect();
    validate_edit(pts, &out, revolve)?;
    Ok(out)
}

// ------------------------------------------------------------------ editing an outline

/// Insert `at` after corner `after` (between it and the next one). False when the outline is full.
pub fn insert_point(pts: &mut Vec<[f32; 2]>, after: usize, at: [f32; 2]) -> bool {
    if pts.len() >= MAX_OUTLINE || after >= pts.len() || !finite(&at) {
        return false;
    }
    pts.insert(after + 1, at);
    true
}

/// Remove corner `i`; at least three stay, and a clean outline stays clean.
pub fn remove_point(pts: &mut Vec<[f32; 2]>, i: usize) -> Result<(), SketchError> {
    if pts.len() <= 3 {
        return Err(SketchError::TooFewPoints);
    }
    if i >= pts.len() {
        return Ok(());
    }
    let was_simple = is_simple(pts);
    let gone = pts.remove(i);
    if was_simple && !is_simple(pts) {
        pts.insert(i, gone);
        return Err(SketchError::SelfIntersect);
    }
    Ok(())
}

/// The nearest point on the closed outline to `p`: (edge index, point on it, distance).
#[cfg(test)]
pub fn nearest_on_edges(pts: &[[f32; 2]], p: [f32; 2]) -> Option<(usize, [f32; 2], f32)> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let mut best: Option<(usize, [f32; 2], f32)> = None;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let l2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-12);
        let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0);
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        let d = dist(p, q);
        if best.is_none_or(|b| d < b.2) {
            best = Some((i, q, d));
        }
    }
    best
}

// ------------------------------------------------------------------ making shapes

/// The numbers of a new shape, ready to put into `Shape::new(kind, pos, ..)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    pub kind: Kind,
    pub pos: V3,
    pub rot: V3,
    pub size: [f32; 3],
    pub outline: Vec<[f32; 2]>,
    pub outline2: Vec<[f32; 2]>,
    pub path: Vec<[f32; 4]>,
    pub params: [f32; 4],
}

fn height_ok(h: f32) -> f32 {
    if h.is_finite() { h.clamp(0.1, 2000.0) } else { 10.0 }
}

fn recentred(p: &[[f32; 2]], c: [f32; 2]) -> Vec<[f32; 2]> {
    p.iter().map(|q| [q[0] - c[0], q[1] - c[1]]).collect()
}

/// A prism of `profile` drawn on `frame`, pushed `height` along the plane's normal (or against it
/// when `flip`). The shape sits at the middle of the profile's box, turned to lie on the plane.
pub fn extrude(frame: &Frame, profile: &[[f32; 2]], height: f32, flip: bool) -> Result<Built, SketchError> {
    let p = validate_profile(profile)?;
    let h = height_ok(height);
    let c = centre(&p);
    let n = frame.normal();
    let lift = if flip { -h * 0.5 } else { h * 0.5 };
    let pos = add(frame.to_world(c), mul(n, lift));
    Ok(Built {
        kind: Kind::Extrude,
        pos,
        rot: frame.euler(),
        size: [h, 1.0, 1.0],
        outline: recentred(&p, c),
        outline2: Vec::new(),
        path: Vec::new(),
        params: Kind::Extrude.default_params(),
    })
}

/// A loft from `bottom` (on the plane) to `top` (the same plane, `height` further along the normal).
pub fn loft(frame: &Frame, bottom: &[[f32; 2]], top: &[[f32; 2]], height: f32, flip: bool) -> Result<Built, SketchError> {
    let b = validate_profile(bottom)?;
    let t = validate_profile(top)?;
    let mut s = extrude(frame, &b, height, flip)?;
    // the bottom profile's middle is the shape's middle; the top is placed relative to the same point
    let c = centre(&b);
    s.kind = Kind::Loft;
    s.outline2 = recentred(&t, c);
    s.params = Kind::Loft.default_params();
    if flip {
        // a flipped loft grows against the normal: its bottom (local -z) is then the far end, so
        // turn the shape over by swapping which profile is where
        std::mem::swap(&mut s.outline, &mut s.outline2);
        // (the profiles were centred on the bottom's box; both are still in the same frame)
    }
    Ok(s)
}

/// A profile drawn on a standing plane (x = distance from the vertical axis through the origin,
/// y = height), turned about that axis.
pub fn revolve(frame: &Frame, profile: &[[f32; 2]]) -> Result<Built, SketchError> {
    if !frame.is_vertical() {
        return Err(SketchError::NeedsVerticalPlane);
    }
    let p = validate_profile(profile)?;
    // the part left of the axis is cut off (as the renderer does); a smooth curve may overshoot a little
    // there, but a profile that mostly lies on the wrong side is a mistake
    if let Some((lo, hi)) = bbox(&p)
        && lo[0] < -0.01
        && -lo[0] > 0.25 * (hi[0] - lo[0])
    {
        return Err(SketchError::LeftOfAxis);
    }
    let clamped: Vec<[f32; 2]> = p.iter().map(|q| [q[0].max(0.0), q[1]]).collect();
    let p = validate_profile(&clamped)?;
    let zc = centre(&p)[1];
    let outline = p.iter().map(|q| [q[0], q[1] - zc]).collect();
    Ok(Built {
        kind: Kind::Revolve,
        pos: frame.to_world([0.0, zc]),
        rot: euler_from_columns(frame.u, mul(frame.normal(), -1.0), frame.v),
        size: Kind::Revolve.default_size(),
        outline,
        outline2: Vec::new(),
        path: Vec::new(),
        params: Kind::Revolve.default_params(),
    })
}

/// The path nodes (x, y, z, radius) for a list of world points, relative to `origin`.
pub fn path_nodes(points: &[V3], origin: V3, radius: f32) -> Vec<[f32; 4]> {
    let r = if radius.is_finite() { radius.clamp(0.1, 500.0) } else { 1.0 };
    points.iter().take(MAX_PATH).map(|p| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2], r]).collect()
}

/// The middle of the box around 3D points, rounded to whole millimetres.
pub fn centre3(points: &[V3]) -> V3 {
    let Some(first) = points.first() else { return [0.0; 3] };
    let (mut lo, mut hi) = (*first, *first);
    for p in points {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let m = |i: usize| ((lo[i] + hi[i]) * 0.5).round();
    [m(0), m(1), m(2)]
}

/// `profile` pulled along `path` (world points). `scale` multiplies the profile (the radius of
/// every path node).
pub fn sweep(profile: &[[f32; 2]], path: &[V3], scale: f32) -> Result<Built, SketchError> {
    let p = validate_profile(profile)?;
    let mut pts: Vec<V3> = Vec::with_capacity(path.len());
    for q in path.iter().take(MAX_PATH) {
        if !q.iter().all(|v| v.is_finite() && v.abs() <= 5000.0) {
            return Err(SketchError::BadNumber);
        }
        if pts.last().is_none_or(|l| len(sub(*l, *q)) > 1e-3) {
            pts.push(*q);
        }
    }
    let total: f32 = pts.windows(2).map(|w| len(sub(w[0], w[1]))).sum();
    if pts.len() < 2 || total < 1.0 {
        return Err(SketchError::PathTooShort);
    }
    let c = centre(&p);
    let pos = centre3(&pts);
    Ok(Built {
        kind: Kind::Sweep,
        pos,
        rot: [0.0; 3],
        size: Kind::Sweep.default_size(),
        outline: recentred(&p, c),
        outline2: Vec::new(),
        path: path_nodes(&pts, pos, scale),
        params: Kind::Sweep.default_params(),
    })
}

/// Put the numbers into a shape (name, size, rotation, outlines, path).
pub fn apply(b: &Built, s: &mut Shape) {
    s.kind = b.kind;
    s.pos = b.pos;
    s.rot = b.rot;
    s.size = b.size;
    s.outline = b.outline.clone();
    s.outline2 = b.outline2.clone();
    s.path = b.path.clone();
    s.params = b.params;
}

// ------------------------------------------------------------------ editing a made shape

/// Which outline of a shape is edited in the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Main,
    /// The loft's top profile.
    Top,
}

/// Is this a shape whose outline can be edited as a sketch?
pub fn editable(s: &Shape) -> bool {
    matches!(s.kind, Kind::Extrude | Kind::Revolve | Kind::Loft | Kind::Sweep)
}

/// The outline of `s` that `which` names.
pub fn outline_of(s: &Shape, which: Which) -> &Vec<[f32; 2]> {
    if which == Which::Top && s.kind == Kind::Loft { &s.outline2 } else { &s.outline }
}

pub fn outline_of_mut(s: &mut Shape, which: Which) -> &mut Vec<[f32; 2]> {
    if which == Which::Top && s.kind == Kind::Loft { &mut s.outline2 } else { &mut s.outline }
}

/// The world frame in which the outline of `s` is drawn, so that editing the outline's numbers
/// moves the points under the mouse: the bottom face of an extrusion, the bottom or top face of a
/// loft, the half-plane of a revolve, the start of a sweep (across its path).
pub fn edit_frame(s: &Shape, which: Which) -> Option<Frame> {
    let to_world = |p: V3| crate::curve_tool::to_world(s, p);
    let (o, u, v) = match s.kind {
        Kind::Extrude => ([0.0, 0.0, -s.size[0] * 0.5], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        Kind::Loft => {
            let z = if which == Which::Top { s.size[0] * 0.5 } else { -s.size[0] * 0.5 };
            ([0.0, 0.0, z], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        }
        Kind::Revolve => ([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        Kind::Sweep => {
            let a = s.path.first()?;
            let b = s.path.get(1)?;
            let tan = norm([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            // the same frame as the distance function in cad.rs
            let up = if tan[2].abs() > 0.99 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
            let nrm = norm(cross(up, tan));
            let bin = cross(tan, nrm);
            let r = a[3].abs().max(1e-3);
            ([a[0], a[1], a[2]], mul(nrm, r), mul(bin, r))
        }
        _ => return None,
    };
    if !(o.iter().chain(u.iter()).chain(v.iter())).all(|x| x.is_finite()) {
        return None;
    }
    let ow = to_world(o);
    let uw = sub(to_world(add(o, u)), ow);
    let vw = sub(to_world(add(o, v)), ow);
    (len(uw) > 1e-6 && len(vw) > 1e-6).then_some(Frame { origin: ow, u: uw, v: vw })
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Material, Scene};
    use crate::sdf::Prepared;

    fn shape_from(b: &Built) -> Shape {
        let mut s = Shape::new(b.kind, [0.0; 3], Material::default());
        apply(b, &mut s);
        s
    }
    fn inside(s: &Shape, w: V3) -> bool {
        Prepared::new(s).distance(w) < 0.0
    }
    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn precise_rectangle_and_circle_dimensions_keep_closed_profiles() {
        let square = rect([-10.0, -5.0], [10.0, 5.0]);
        assert_eq!(rectangle_size(&square), Some([20.0, 10.0]));
        let r = set_rectangle_size(&square, 30.0, 12.0, false).unwrap();
        assert_eq!(rectangle_size(&r), Some([30.0, 12.0]));
        assert_eq!(centre(&r), centre(&square));
        assert_eq!(r.len(), 4);
        assert!(validate_profile(&r).is_ok());
        assert_eq!(set_rectangle_size(&square, 0.01, 10.0, false), Err(SketchError::TooSmall));
        assert_eq!(set_rectangle_size(&square, f32::NAN, 10.0, false), Err(SketchError::BadNumber));

        let ring = circle([3.0, 4.0], 10.0, CIRCLE_POINTS);
        assert!((circle_diameter(&ring).unwrap() - 20.0).abs() < 1e-3);
        let enlarged = set_circle_diameter(&ring, 40.0, false).unwrap();
        assert!((circle_diameter(&enlarged).unwrap() - 40.0).abs() < 1e-3);
        assert_eq!(enlarged.len(), CIRCLE_POINTS);
        assert!(validate_profile(&enlarged).is_ok());
        assert_eq!(set_circle_diameter(&ring, 0.2, false), Err(SketchError::TooSmall));
    }

    #[test]
    fn precise_vertex_and_edge_edits_cannot_break_topology() {
        let square = rect([0.0, 0.0], [20.0, 20.0]);
        let changed = move_vertex(&square, 2, [25.0, 25.0], false).unwrap();
        assert_eq!(changed[2], [25.0, 25.0]);
        assert_eq!(changed.len(), square.len());
        let longer = set_edge_length(&square, 0, 25.0, false).unwrap();
        assert!((dist(longer[0], longer[1]) - 25.0).abs() < 1e-4);
        assert!(validate_profile(&longer).is_ok());
        assert!(move_vertex(&square, 2, [0.0, 0.0], false).is_err());
        assert!(move_vertex(&square, 2, [20.0, -10.0], false).is_err());
        assert_eq!(move_vertex(&square, 0, [f32::NAN, 0.0], false), Err(SketchError::BadNumber));
        assert_eq!(move_vertex(&square, 0, [-1.0, 0.0], true), Err(SketchError::LeftOfAxis));
        assert_eq!(set_edge_length(&square, 0, 0.0, false), Err(SketchError::TooSmall));
    }

    #[test]
    fn frames_map_points_both_ways_and_face_the_viewer() {
        for f in [Frame::floor(), Frame::front(), Frame::side(), Frame::horizontal(12.0)] {
            let p = [7.0, -3.5];
            let w = f.to_world(p);
            let q = f.from_world(w);
            assert!(near(p[0], q[0]) && near(p[1], q[1]));
        }
        assert_eq!(Frame::floor().normal(), [0.0, 0.0, 1.0]);
        assert_eq!(Frame::front().normal(), [0.0, -1.0, 0.0], "the front plane faces the viewer at -Y");
        assert_eq!(Frame::side().normal(), [1.0, 0.0, 0.0]);
        assert!(Frame::front().is_vertical() && Frame::side().is_vertical());
        assert!(!Frame::floor().is_vertical() && !Frame::horizontal(5.0).is_vertical());
        assert_eq!(Frame::horizontal(f32::NAN).origin[2], 0.0);
    }

    #[test]
    fn a_ray_meets_the_plane_where_you_would_expect() {
        let f = Frame::horizontal(10.0);
        // straight down from above onto (5, 6)
        let p = f.hit([5.0, 6.0, 100.0], [0.0, 0.0, -1.0]).expect("hit");
        assert!(near(p[0], 5.0) && near(p[1], 6.0));
        assert!(f.hit([0.0, 0.0, 100.0], [1.0, 0.0, 0.0]).is_none(), "parallel");
        assert!(f.hit([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]).is_none(), "plane behind the ray");
        assert!(f.hit([0.0, 0.0, 100.0], [0.0, 0.001, -1.0]).is_some());
        assert!(f.hit([1e9, 0.0, 100.0], [0.0, 0.0, -1.0]).is_none(), "absurdly far");
        assert!(f.hit([f32::NAN, 0.0, 100.0], [0.0, 0.0, -1.0]).is_none());
        let front = Frame::front();
        let p = front.hit([3.0, -50.0, 8.0], [0.0, 1.0, 0.0]).expect("hit");
        assert!(near(p[0], 3.0) && near(p[1], 8.0));
    }

    #[test]
    fn euler_angles_turn_a_shape_onto_its_plane() {
        for f in [Frame::floor(), Frame::front(), Frame::side(), Frame::horizontal(4.0)] {
            let e = f.euler();
            let r = rotation(e);
            // local x, y, z map to u, v, normal
            for (col, want) in [(0, f.u), (1, f.v), (2, f.normal())] {
                let got = [r[0][col], r[1][col], r[2][col]];
                assert!(len(sub(got, want)) < 1e-4, "{e:?} column {col}: {got:?} vs {want:?}");
            }
        }
        // looking along an axis (gimbal lock) still works
        let tilted = euler_from_columns([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        let r = rotation(tilted);
        assert!(len(sub([r[0][0], r[1][0], r[2][0]], [0.0, 0.0, -1.0])) < 1e-4);
    }

    #[test]
    fn arbitrary_workplanes_have_stable_inverse_transforms() {
        let tilted = Frame::from_axes([32.0, -18.0, 70.0], [0.5, 0.4, 0.8], [-0.6, 0.8, 0.3]).expect("tilt");
        let frames = [
            Frame::floor(),
            Frame::front(),
            Frame::side(),
            Frame::horizontal(-83.25),
            tilted,
            Frame::oriented([3.0, 8.0, -42.0], [17.0, -31.0, 73.0]).expect("rotated"),
            Frame::oriented([-20.0, 14.0, 60.0], [85.0, 89.9, -130.0]).expect("near gimbal lock"),
        ];
        for frame in frames {
            assert!((len(frame.u) - 1.0).abs() < 1e-5);
            assert!((len(frame.v) - 1.0).abs() < 1e-5);
            assert!(dot(frame.u, frame.v).abs() < 1e-5);
            for local in [[0.0, 0.0], [15.25, -48.5], [-97.0, 32.125]] {
                let world = frame.to_world(local);
                let recovered = frame.from_world(world);
                assert!((recovered[0] - local[0]).abs() < 0.002 && (recovered[1] - local[1]).abs() < 0.002, "{frame:?}: {local:?} -> {recovered:?}");
                let normal = frame.normal();
                let ray_origin = add(world, mul(normal, 200.0));
                let hit = frame.hit(ray_origin, mul(normal, -1.0)).expect("ray hit");
                assert!((hit[0] - local[0]).abs() < 0.002 && (hit[1] - local[1]).abs() < 0.002);
            }
            let reloaded = Frame::oriented(frame.origin, frame.euler()).expect("saved rotation");
            assert!(len(sub(frame.u, reloaded.u)) < 0.002 && len(sub(frame.v, reloaded.v)) < 0.002, "rotation roundtrip: {frame:?} -> {reloaded:?}");
        }
    }

    #[test]
    fn invalid_workplanes_are_rejected_without_nan_coordinates() {
        assert!(Frame::from_axes([0.0; 3], [0.0; 3], [0.0, 1.0, 0.0]).is_none());
        assert!(Frame::from_axes([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]).is_none());
        assert!(Frame::from_axes([f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]).is_none());
        assert!(Frame::oriented([0.0; 3], [f32::INFINITY, 0.0, 0.0]).is_none());
        assert!(Frame::oriented([1.0e9, 0.0, 0.0], [0.0; 3]).is_none());
    }

    #[test]
    fn segment_snap_intersections_are_finite_and_reject_parallel_overlaps() {
        let cross = snap_intersection([0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0]);
        assert_eq!(cross, Some([5.0, 5.0]));
        assert_eq!(snap_intersection([0.0, 0.0], [10.0, 0.0], [5.0, 0.0], [15.0, 0.0]), None);
        assert_eq!(snap_intersection([0.0, 0.0], [10.0, 0.0], [10.0, 0.0], [10.0, 8.0]), None);
        assert_eq!(snap_intersection([f32::NAN, 0.0], [10.0, 0.0], [5.0, -5.0], [5.0, 5.0]), None);
    }

    #[test]
    fn snap_geometry_collects_endpoints_midpoints_and_centers_without_dragged_vertex() {
        let mut geo = SnapGeometry::default();
        geo.add_outline(&[[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]], true, None);
        assert_eq!(geo.edges.len(), 4);
        assert!(geo.points.iter().any(|h| h.kind == SnapKind::Center && h.at == [10.0, 5.0]));
        assert!(geo.points.iter().any(|h| h.kind == SnapKind::Midpoint && h.at == [10.0, 0.0]));
        let mut edit = SnapGeometry::default();
        edit.add_outline(&[[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]], true, Some(0));
        assert!(!edit.points.iter().any(|h| h.at == [0.0, 0.0]));
        assert_eq!(edit.edges.len(), 2);
    }

    #[test]
    fn screen_snap_priorities_zoom_and_hysteresis_are_stable() {
        let a = SnapHit { at: [0.0, 0.0], kind: SnapKind::Origin };
        let b = SnapHit { at: [1.0, 1.0], kind: SnapKind::Grid };
        let hits = vec![(a, [104.0, 100.0]), (b, [100.0, 100.0])];
        assert_eq!(choose_screen_snap([100.0, 100.0], &hits, None), Some(a));
        assert_eq!(choose_screen_snap([100.0, 100.0], &[(b, [100.0, 100.0])], Some(a)), Some(b));
        // Held target stays while inside release radius, unless the alternative is clearly closer.
        let swapped = vec![(a, [112.0, 100.0]), (b, [108.0, 100.0])];
        assert_eq!(choose_screen_snap([100.0, 100.0], &swapped, Some(a)), Some(a));
        assert_eq!(choose_screen_snap([90.0, 100.0], &hits, Some(a)), Some(a));
        assert_eq!(choose_screen_snap([200.0, 100.0], &hits, Some(a)), None);
        // Equal pixel locations behave identically irrespective of underlying millimetres.
        let far = SnapHit { at: [1000.0, 800.0], kind: SnapKind::Endpoint };
        assert_eq!(choose_screen_snap([0.0, 0.0], &[(far, [8.0, 0.0])], None), Some(far));
        assert_eq!(choose_screen_snap([0.0, 0.0], &[(far, [12.0, 0.0])], None), None);
    }

    #[test]
    fn angle_snap_follows_the_nearest_fifteen_degree_ray() {
        let p = snap_angle([3.0, 4.0], [13.0, 6.0], 15.0).unwrap();
        assert!((p[1] - 4.0 - (p[0] - 3.0) * 15f32.to_radians().tan()).abs() < 1e-3);
        assert_eq!(snap_angle([0.0, 0.0], [0.0, 0.0], 15.0), None);
        assert_eq!(snap_angle([0.0, 0.0], [5.0, 3.0], f32::NAN), None);
    }

    #[test]
    fn snapping_rounds_to_the_grid_and_ignores_bad_steps() {
        assert_eq!(snap([1.4, -2.6], 1.0), [1.0, -3.0]);
        assert_eq!(snap([12.4, 7.6], 10.0), [10.0, 10.0]);
        assert_eq!(snap([0.34, 0.26], 0.1), [0.3, 0.3]);
        assert_eq!(snap([1.4, 2.0], 0.0), [1.0, 2.0]);
        assert_eq!(snap([1.4, 2.0], f32::NAN), [1.0, 2.0]);
    }

    #[test]
    fn rectangles_and_circles_are_counter_clockwise_and_simple() {
        let r = rect([10.0, 8.0], [-2.0, -4.0]);
        assert_eq!(r, vec![[-2.0, -4.0], [10.0, -4.0], [10.0, 8.0], [-2.0, 8.0]]);
        assert!(near(signed_area(&r), 144.0));
        let c = circle([5.0, 5.0], 10.0, CIRCLE_POINTS);
        assert_eq!(c.len(), 32);
        assert!(signed_area(&c) > 0.0 && is_simple(&c));
        // area of a 32-gon of radius 10
        assert!((signed_area(&c) - 0.5 * 32.0 * 100.0 * (std::f32::consts::TAU / 32.0).sin()).abs() < 0.1);
        assert!(rect_profile([0.0, 0.0], [0.2, 10.0]).is_err(), "a sliver is no rectangle");
        assert_eq!(rect_profile([0.0, 0.0], [10.0, 10.0]).map(|p| p.len()), Ok(4));
        assert_eq!(circle_profile([0.0; 2], [0.1, 0.0]), Err(SketchError::TooSmall));
        assert_eq!(circle_profile([0.0; 2], [4.0, 0.0]).map(|p| p.len()), Ok(32));
        assert_eq!(circle([0.0; 2], 1.0, 10_000).len(), MAX_OUTLINE, "a circle never exceeds the outline limit");
    }

    #[test]
    fn winding_is_fixed_and_degenerate_outlines_are_refused() {
        let cw = vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]];
        assert!(signed_area(&cw) < 0.0);
        let fixed = validate_profile(&cw).expect("a clockwise square is fine");
        assert!(signed_area(&fixed) > 0.0, "turned counter-clockwise");
        // repeated points and a closing duplicate go away
        let dup = vec![[0.0, 0.0], [0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]];
        assert_eq!(validate_profile(&dup).map(|p| p.len()), Ok(4));
        assert_eq!(validate_profile(&[[0.0, 0.0], [5.0, 5.0]]), Err(SketchError::TooFewPoints));
        assert_eq!(validate_profile(&[[0.0, 0.0], [5.0, 5.0], [10.0, 10.0]]), Err(SketchError::TooSmall), "collinear = no area");
        assert_eq!(validate_profile(&[[0.0, 0.0], [f32::NAN, 5.0], [10.0, 10.0]]), Err(SketchError::BadNumber));
        assert_eq!(validate_profile(&[[0.0, 0.0], [1e9, 5.0], [10.0, 10.0]]), Err(SketchError::BadNumber));
        let many: Vec<[f32; 2]> = (0..MAX_OUTLINE + 1)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / (MAX_OUTLINE + 1) as f32;
                [50.0 * a.cos(), 50.0 * a.sin()]
            })
            .collect();
        assert_eq!(validate_profile(&many), Err(SketchError::TooManyPoints));
        assert!(!SketchError::SelfIntersect.message().is_empty());
    }

    #[test]
    fn holes_accept_concentric_eccentric_rectangular_and_separate_profiles() {
        let outer = rect([-30.0, -30.0], [30.0, 30.0]);
        let centre = circle([0.0, 0.0], 6.0, CIRCLE_POINTS);
        let offset = circle([16.0, 0.0], 3.0, CIRCLE_POINTS);
        let rectangular = rect([-25.0, -4.0], [-16.0, 4.0]);
        assert_eq!(validate_holes(&outer, std::slice::from_ref(&centre)).unwrap().len(), 1);
        assert_eq!(validate_holes(&outer, std::slice::from_ref(&offset)).unwrap().len(), 1);
        assert_eq!(validate_holes(&outer, std::slice::from_ref(&rectangular)).unwrap().len(), 1);
        assert_eq!(validate_holes(&outer, &[centre, offset, rectangular]).unwrap().len(), 3);
    }

    #[test]
    fn holes_reject_contact_crossing_nesting_overlap_and_outside() {
        let outer = rect([-20.0, -20.0], [20.0, 20.0]);
        assert_eq!(validate_holes(&outer, &[rect([20.0, -2.0], [24.0, 2.0])]), Err(SketchError::ContoursTouch));
        assert_eq!(validate_holes(&outer, &[rect([19.0, -2.0], [24.0, 2.0])]), Err(SketchError::ContoursTouch));
        assert_eq!(validate_holes(&outer, &[rect([25.0, -2.0], [30.0, 2.0])]), Err(SketchError::HoleOutside));
        assert_eq!(validate_holes(&outer, &[rect([-10.0, -10.0], [10.0, 10.0]), rect([-2.0, -2.0], [2.0, 2.0])]), Err(SketchError::HoleOverlap),);
        assert_eq!(validate_holes(&outer, &[rect([-8.0, -8.0], [2.0, 2.0]), rect([2.0, -5.0], [8.0, 5.0])]), Err(SketchError::ContoursTouch),);
        assert_eq!(validate_holes(&outer, &[rect([-8.0, -8.0], [2.0, 2.0]), rect([0.0, -5.0], [8.0, 5.0])]), Err(SketchError::ContoursTouch),);
        assert_eq!(validate_holes(&outer, &[rect([-10.0, -3.0], [-9.98, 3.0])]), Err(SketchError::TooSmall));
        assert_eq!(validate_holes(&outer, &vec![rect([-1.0, -1.0], [1.0, 1.0]); MAX_HOLES + 1]), Err(SketchError::TooManyHoles),);
    }

    #[test]
    fn crossing_outlines_are_found() {
        // a bow tie
        let bow = vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
        assert!(!is_simple(&bow));
        assert_eq!(validate_profile(&bow), Err(SketchError::SelfIntersect));
        // an L shape and a star are fine
        let l = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 5.0], [5.0, 5.0], [5.0, 20.0], [0.0, 20.0]];
        assert!(is_simple(&l));
        let star: Vec<[f32; 2]> = (0..10)
            .map(|i| {
                let a = i as f32 * std::f32::consts::PI / 5.0;
                let r = if i % 2 == 0 { 20.0 } else { 8.0 };
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        assert!(is_simple(&star));
        // an edge that touches another corner is a pinch: refused
        let pinch = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [10.0, 0.0], [10.0, -10.0], [0.0, -10.0]];
        assert!(!is_simple(&pinch));
        // a spike that folds back over itself
        let spike = vec![[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        assert!(!is_simple(&spike));
        assert!(!is_simple(&[[0.0, 0.0], [1.0, 1.0]]));
        assert!(segments_meet([0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0]));
        assert!(!segments_meet([0.0, 0.0], [10.0, 0.0], [0.0, 1.0], [10.0, 1.0]));
        assert!(segments_meet([0.0, 0.0], [10.0, 0.0], [10.0, 0.0], [20.0, 5.0]), "touching ends meet");
    }

    #[test]
    fn a_new_segment_is_checked_against_the_line_so_far() {
        let line = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0]];
        // crossing the first segment
        assert!(crosses_polyline(&line, [20.0, 10.0], [10.0, -10.0], false));
        // going away is fine
        assert!(!crosses_polyline(&line, [20.0, 10.0], [0.0, 10.0], false));
        // closing back onto the first point is allowed
        assert!(!crosses_polyline(&line, [20.0, 10.0], [0.0, 0.0], true));
        // folding back over the last segment is not
        assert!(crosses_polyline(&line, [20.0, 10.0], [20.0, 5.0], false));
        assert!(!crosses_polyline(&[[0.0, 0.0]], [5.0, 5.0], [9.0, 0.0], false));
        // closing over an earlier segment is not
        let hook = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [30.0, 10.0]];
        assert!(crosses_polyline(&hook, [30.0, 10.0], [0.0, 0.0], true));
    }

    #[test]
    fn a_spline_passes_through_its_points_stays_small_and_closes() {
        let ctrl = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]];
        let s = spline_closed(&ctrl, MAX_OUTLINE);
        assert!(s.len() <= MAX_OUTLINE && s.len() >= ctrl.len() * 4, "{} points", s.len());
        // every control point is on the curve
        for c in &ctrl {
            assert!(s.iter().any(|p| dist(*p, *c) < 1e-3), "passes through {c:?}");
        }
        // closed: the curve is a simple loop and wraps from the last sample back to the first
        assert!(is_simple(&s) && signed_area(&s) > 0.0);
        let step = |a: [f32; 2], b: [f32; 2]| dist(a, b);
        let wrap = step(s[s.len() - 1], s[0]);
        let typical = s.windows(2).map(|w| step(w[0], w[1])).fold(0.0_f32, f32::max);
        assert!(wrap <= typical * 1.5, "no jump at the seam: {wrap} vs {typical}");
        // many points: still within the limit (down to one sample per segment)
        for n in [3, 7, 20, 40, 64, 200] {
            let ring: Vec<[f32; 2]> = (0..n)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / n as f32;
                    [60.0 * a.cos(), 60.0 * a.sin()]
                })
                .collect();
            let s = spline_closed(&ring, MAX_OUTLINE);
            assert!(s.len() <= MAX_OUTLINE, "n={n}: {}", s.len());
        }
        assert!(spline_closed(&[[0.0, 0.0], [1.0, 1.0]], 64).len() <= 2, "too few to curve");
        assert!(spline_closed(&[[f32::NAN, 0.0], [1.0, 1.0], [3.0, 0.0], [4.0, 4.0]], 64).iter().all(|p| p[0].is_finite()));
        // the smooth profile is valid and has the limit respected
        let prof = closed_profile(&ctrl, true).expect("valid");
        assert!(prof.len() <= MAX_OUTLINE);
        assert_eq!(closed_profile(&ctrl, false).map(|p| p.len()), Ok(4), "corners stay as clicked");
        // overshooting points that make the curve loop through itself are refused, not crashed on
        let tight = vec![[0.0, 0.0], [40.0, 0.0], [1.0, 1.0], [40.0, 1.5], [0.0, 2.0]];
        let _ = closed_profile(&tight, true);
    }

    #[test]
    fn an_open_spline_keeps_its_ends_and_the_path_limit() {
        let ctrl = vec![[0.0, 0.0], [20.0, 10.0], [40.0, -5.0], [60.0, 8.0]];
        let s = spline_open(&ctrl, MAX_PATH);
        assert!(s.len() <= MAX_PATH && s.len() > ctrl.len());
        assert_eq!(s.first(), ctrl.first());
        assert_eq!(s.last(), ctrl.last());
        for c in &ctrl {
            assert!(s.iter().any(|p| dist(*p, *c) < 1e-3));
        }
        for n in [2, 3, 10, 47, 48, 100] {
            let c: Vec<[f32; 2]> = (0..n).map(|i| [i as f32 * 5.0, (i as f32).sin() * 9.0]).collect();
            let l = open_line(&c, true);
            assert!(l.as_ref().map(|v| v.len() <= MAX_PATH).unwrap_or(true), "n={n}");
            let l = open_line(&c, false);
            assert!(l.as_ref().map(|v| v.len() <= MAX_PATH).unwrap_or(true), "n={n} corners");
        }
        assert_eq!(open_line(&[[0.0, 0.0]], true), Err(SketchError::PathTooShort));
        assert_eq!(open_line(&[[0.0, 0.0], [0.2, 0.0]], false), Err(SketchError::PathTooShort));
        assert_eq!(open_line(&[[0.0, 0.0], [f32::NAN, 0.0], [5.0, 5.0]], true), Err(SketchError::BadNumber));
    }

    #[test]
    fn scaling_a_profile_keeps_its_middle() {
        let sq = rect([0.0, 0.0], [10.0, 10.0]);
        let half = scaled(&sq, 0.5);
        assert_eq!(bbox(&half), Some(([2.5, 2.5], [7.5, 7.5])));
        let big = scaled(&sq, 2.0);
        assert_eq!(bbox(&big), Some(([-5.0, -5.0], [15.0, 15.0])));
        assert_eq!(scaled(&sq, f32::NAN), sq);
        assert!(scaled(&sq, 0.0).iter().all(|p| p[0] > 4.0 && p[0] < 6.0), "scale 0 is held to a small positive number");
        assert!(scaled(&[], 2.0).is_empty());
    }

    #[test]
    fn an_extrusion_lands_on_every_plane_the_right_way_round() {
        // a 20 x 10 profile whose lower-left corner is at (30, 5) of the plane, 8 mm tall
        let prof = rect([30.0, 5.0], [50.0, 15.0]);
        for (name, f) in [("floor", Frame::floor()), ("front", Frame::front()), ("side", Frame::side()), ("on a shape", Frame::horizontal(25.0))] {
            let b = extrude(&f, &prof, 8.0, false).expect("builds");
            let s = shape_from(&b);
            let n = f.normal();
            let mid = f.to_world([40.0, 10.0]);
            assert!(inside(&s, add(mid, mul(n, 4.0))), "{name}: middle of the solid");
            assert!(inside(&s, add(f.to_world([31.0, 6.0]), mul(n, 0.5))), "{name}: near the corner (31, 6)");
            assert!(inside(&s, add(f.to_world([49.0, 14.0]), mul(n, 7.5))), "{name}: near the far corner, almost at the top");
            assert!(!inside(&s, add(mid, mul(n, -1.0))), "{name}: nothing below the plane");
            assert!(!inside(&s, add(mid, mul(n, 9.0))), "{name}: nothing above the height");
            assert!(!inside(&s, add(f.to_world([29.0, 10.0]), mul(n, 4.0))), "{name}: outside to the left");
            assert!(!inside(&s, add(f.to_world([40.0, 16.0]), mul(n, 4.0))), "{name}: outside above the outline");
            // the whole thing is a solid 20 x 10 x 8 box
            assert!(near(s.size[0], 8.0));
        }
        // flipped: it grows the other way
        let f = Frame::floor();
        let s = shape_from(&extrude(&f, &prof, 8.0, true).expect("builds"));
        assert!(inside(&s, [40.0, 10.0, -4.0]) && !inside(&s, [40.0, 10.0, 4.0]));
        // hostile heights are made sane
        let h = extrude(&f, &prof, f32::NAN, false).expect("builds");
        assert!((0.1..=2000.0).contains(&h.size[0]));
        let h = extrude(&f, &prof, -50.0, false).expect("builds");
        assert!(h.size[0] >= 0.1);
        assert!(extrude(&f, &[[0.0, 0.0]], 5.0, false).is_err());
    }

    #[test]
    fn the_outline_is_centred_and_the_shape_sits_at_its_middle() {
        let b = extrude(&Frame::floor(), &rect([30.0, 5.0], [50.0, 15.0]), 8.0, false).expect("builds");
        assert_eq!(b.pos, [40.0, 10.0, 4.0]);
        assert_eq!(bbox(&b.outline), Some(([-10.0, -5.0], [10.0, 5.0])));
        assert!(signed_area(&b.outline) > 0.0);
        // it round-trips through the file format
        let mut sc = Scene::default();
        sc.groups[0].shapes.push(shape_from(&b));
        let back = Scene::from_json(&sc.to_json()).expect("loads");
        assert_eq!(back.groups[0].shapes[1].outline, b.outline);
    }

    #[test]
    fn a_loft_goes_from_the_drawn_profile_to_the_top_profile() {
        let bottom = rect([-10.0, -10.0], [10.0, 10.0]);
        let top = scaled(&bottom, 0.4);
        for f in [Frame::floor(), Frame::front(), Frame::side()] {
            let s = shape_from(&loft(&f, &bottom, &top, 30.0, false).expect("builds"));
            let n = f.normal();
            let at = |x: f32, y: f32, h: f32| add(f.to_world([x, y]), mul(n, h));
            assert!(inside(&s, at(9.0, 9.0, 1.0)), "wide near the bottom");
            assert!(!inside(&s, at(9.0, 9.0, 29.0)), "narrow near the top");
            assert!(inside(&s, at(3.0, 3.0, 29.0)), "but the middle reaches the top");
            assert!(!inside(&s, at(0.0, 0.0, 31.0)) && !inside(&s, at(0.0, 0.0, -1.0)));
        }
        // flipped: bottom profile is still the one on the plane, growing the other way
        let f = Frame::floor();
        let s = shape_from(&loft(&f, &bottom, &top, 30.0, true).expect("builds"));
        assert!(inside(&s, [9.0, 9.0, -1.0]), "the drawn profile is at the plane");
        assert!(!inside(&s, [9.0, 9.0, -29.0]) && inside(&s, [3.0, 3.0, -29.0]));
        // a scaled copy of a triangle keeps a valid outline
        let tri = vec![[0.0, 0.0], [20.0, 0.0], [10.0, 17.0]];
        assert!(loft(&f, &tri, &scaled(&tri, 1.5), 10.0, false).is_ok());
        assert!(loft(&f, &tri, &[[0.0, 0.0]], 10.0, false).is_err());
    }

    #[test]
    fn a_revolve_turns_the_profile_about_the_vertical_axis() {
        // a profile 10..20 from the axis and 5..35 high drawn on the front plane
        let prof = rect([10.0, 5.0], [20.0, 35.0]);
        for f in [Frame::front(), Frame::side()] {
            let b = revolve(&f, &prof).expect("builds");
            let s = shape_from(&b);
            assert!(inside(&s, [15.0, 0.0, 20.0]) && inside(&s, [0.0, 15.0, 20.0]) && inside(&s, [-15.0, 0.0, 20.0]), "a ring in every direction");
            assert!(inside(&s, [10.6, 10.6, 20.0]), "diagonal too (radius 15)");
            assert!(!inside(&s, [0.0, 0.0, 20.0]), "the middle is empty");
            assert!(!inside(&s, [15.0, 0.0, 40.0]) && !inside(&s, [15.0, 0.0, 2.0]));
            assert!(!inside(&s, [25.0, 0.0, 20.0]));
            assert_eq!(b.params[0], 360.0);
        }
        assert_eq!(revolve(&Frame::floor(), &prof), Err(SketchError::NeedsVerticalPlane));
        assert_eq!(revolve(&Frame::front(), &rect([-10.0, 0.0], [10.0, 20.0])), Err(SketchError::LeftOfAxis));
        // a profile touching the axis (x = 0, even -0.001) is a solid of revolution
        let solid = revolve(&Frame::front(), &[[-0.001, 0.0], [12.0, 0.0], [8.0, 30.0], [0.0, 30.0]]).expect("builds");
        let s = shape_from(&solid);
        assert!(inside(&s, [0.5, 0.0, 15.0]) && inside(&s, [0.0, 7.0, 3.0]));
        assert!(solid.outline.iter().all(|p| p[0] >= 0.0));
        // a smooth curve that overshoots the axis a little is trimmed to it
        let vase = closed_profile(&[[0.0, 0.0], [18.0, 0.0], [22.0, 20.0], [12.0, 40.0], [14.0, 55.0], [0.0, 55.0]], true).expect("a smooth outline");
        assert!(vase.iter().any(|q| q[0] < -0.01), "the curve does swing out to the left");
        let b = revolve(&Frame::front(), &vase).expect("trimmed");
        assert!(b.outline.iter().all(|q| q[0] >= 0.0) && is_simple(&b.outline));
    }

    #[test]
    fn a_sweep_follows_the_path_in_world_space() {
        // a 10 x 10 square pulled up a straight vertical path from z = 5 to z = 45 at (20, 30)
        let prof = rect([-5.0, -5.0], [5.0, 5.0]);
        let path = vec![[20.0, 30.0, 5.0], [20.0, 30.0, 45.0]];
        let b = sweep(&prof, &path, 1.0).expect("builds");
        assert_eq!(b.pos, [20.0, 30.0, 25.0]);
        let s = shape_from(&b);
        assert!(inside(&s, [20.0, 30.0, 25.0]) && inside(&s, [23.0, 27.0, 40.0]));
        assert!(!inside(&s, [20.0, 30.0, 50.0]) && !inside(&s, [30.0, 30.0, 25.0]));
        // a bigger scale fattens it
        let fat = shape_from(&sweep(&prof, &path, 2.0).expect("builds"));
        assert!(inside(&fat, [28.0, 30.0, 25.0]) && !inside(&s, [28.0, 30.0, 25.0]));
        // a path along X (drawn on the front plane): the profile's x runs along Y, its y along Z
        let f = Frame::front();
        let arc: Vec<V3> = [[0.0, 10.0], [40.0, 10.0]].iter().map(|p| f.to_world(*p)).collect();
        let s = shape_from(&sweep(&rect([-3.0, -2.0], [3.0, 2.0]), &arc, 1.0).expect("builds"));
        assert!(inside(&s, [20.0, 2.5, 10.0]) && !inside(&s, [20.0, 0.0, 14.0]) && inside(&s, [20.0, 0.0, 11.5]));
        // too short, too few, bad numbers
        assert_eq!(sweep(&prof, &[[0.0; 3]], 1.0), Err(SketchError::PathTooShort));
        assert_eq!(sweep(&prof, &[[0.0; 3], [0.0, 0.0, 0.4]], 1.0), Err(SketchError::PathTooShort));
        assert_eq!(sweep(&prof, &[[0.0; 3], [f32::NAN, 0.0, 9.0]], 1.0), Err(SketchError::BadNumber));
        let r = sweep(&prof, &path, f32::NAN).expect("builds");
        assert!(r.path.iter().all(|n| n[3] == 1.0));
    }

    #[test]
    fn a_long_path_is_cut_to_the_node_limit() {
        let pts: Vec<V3> = (0..200).map(|i| [i as f32 * 2.0, (i as f32 * 0.2).sin() * 10.0, 5.0]).collect();
        let b = sweep(&rect([-3.0, -3.0], [3.0, 3.0]), &pts, 1.0).expect("builds");
        assert_eq!(b.path.len(), MAX_PATH);
        let nodes = path_nodes(&pts, [0.0; 3], 1.0);
        assert_eq!(nodes.len(), MAX_PATH);
        assert_eq!(centre3(&[]), [0.0; 3]);
    }

    #[test]
    fn the_edit_frame_matches_where_the_outline_is_drawn() {
        // an extrusion made on a side plane: its outline lies on the plane at its bottom face
        let f = Frame::side();
        let prof = rect([30.0, 5.0], [50.0, 15.0]);
        let s = shape_from(&extrude(&f, &prof, 8.0, false).expect("builds"));
        let ef = edit_frame(&s, Which::Main).expect("frame");
        // outline point (0, 0) is the middle of the profile's box
        let w = ef.to_world([0.0, 0.0]);
        assert!(len(sub(w, f.to_world([40.0, 10.0]))) < 1e-3, "{w:?}");
        // and the normal points the way the solid grows (it is also a point on the base plane)
        assert!(dot(ef.normal(), f.normal()) > 0.99);
        // screen hits turn back into outline coordinates
        let ray_o = add(f.to_world([45.0, 12.0]), mul(f.normal(), 100.0));
        let q = ef.hit(ray_o, mul(f.normal(), -1.0)).expect("hit");
        assert!(near(q[0], 5.0) && near(q[1], 2.0), "{q:?}");
        // a loft's top frame is one height higher
        let l =
            shape_from(&loft(&Frame::floor(), &rect([-10.0, -10.0], [10.0, 10.0]), &rect([-5.0, -5.0], [5.0, 5.0]), 20.0, false).expect("builds"));
        let (lo, hi) = (edit_frame(&l, Which::Main).expect("f"), edit_frame(&l, Which::Top).expect("f"));
        assert!(near(lo.origin[2], 0.0) && near(hi.origin[2], 20.0));
        // a revolve's outline stands on the XZ plane, x = distance from the axis
        let r = shape_from(&revolve(&Frame::front(), &rect([10.0, 5.0], [20.0, 35.0])).expect("builds"));
        let rf = edit_frame(&r, Which::Main).expect("frame");
        for p in &r.outline {
            let w = rf.to_world(*p);
            assert!(near(w[1], 0.0) && near(w[0], p[0]) && near(w[2], p[1] + r.pos[2]), "{w:?}");
        }
        // a sweep's outline is across the start of its path
        let sw = shape_from(&sweep(&rect([-5.0, -5.0], [5.0, 5.0]), &[[20.0, 30.0, 5.0], [20.0, 30.0, 45.0]], 1.0).expect("builds"));
        let sf = edit_frame(&sw, Which::Main).expect("frame");
        assert!(near(sf.origin[2], 5.0) && near(sf.normal()[2].abs(), 1.0), "{sf:?}");
        // other kinds have no outline to edit
        let ball = Shape::new(Kind::Sphere, [0.0; 3], Material::default());
        assert!(edit_frame(&ball, Which::Main).is_none() && !editable(&ball));
        assert!(editable(&s));
        // a scaled shape: the frame's axes are longer so outline coordinates stay the shape's own
        let mut big = s.clone();
        big.scale = [2.0, 2.0, 1.0];
        let bf = edit_frame(&big, Which::Main).expect("frame");
        let w = bf.to_world([5.0, 0.0]);
        let back = bf.from_world(w);
        assert!(near(back[0], 5.0));
    }

    #[test]
    fn points_are_inserted_removed_and_kept_above_three() {
        let mut sq = rect([0.0, 0.0], [10.0, 10.0]);
        assert!(insert_point(&mut sq, 0, [5.0, -2.0]));
        assert_eq!(sq.len(), 5);
        assert_eq!(sq[1], [5.0, -2.0]);
        assert!(is_simple(&sq));
        assert!(!insert_point(&mut sq, 99, [1.0, 1.0]));
        assert!(!insert_point(&mut sq, 0, [f32::NAN, 1.0]));
        assert_eq!(remove_point(&mut sq, 1), Ok(()));
        assert_eq!(sq.len(), 4);
        assert_eq!(remove_point(&mut sq, 0), Ok(()));
        assert_eq!(sq.len(), 3);
        assert_eq!(remove_point(&mut sq, 0), Err(SketchError::TooFewPoints), "three points stay");
        assert_eq!(sq.len(), 3);
        // removing a corner that would make the outline cross is refused
        let mut w = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [12.0, 5.0], [8.0, 5.0], [0.0, 20.0]];
        assert!(is_simple(&w));
        let before = w.clone();
        if remove_point(&mut w, 4).is_err() {
            assert_eq!(w, before);
        }
        let big: Vec<[f32; 2]> = (0..MAX_OUTLINE)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / MAX_OUTLINE as f32;
                [30.0 * a.cos(), 30.0 * a.sin()]
            })
            .collect();
        let mut full = big;
        assert!(!insert_point(&mut full, 0, [0.0, 0.0]), "never past the limit");
        let (i, q, d) = nearest_on_edges(&rect([0.0, 0.0], [10.0, 10.0]), [5.0, -3.0]).expect("edge");
        assert_eq!((i, q), (0, [5.0, 0.0]));
        assert!(near(d, 3.0));
        assert!(nearest_on_edges(&[], [0.0, 0.0]).is_none());
    }

    #[test]
    fn the_top_of_a_shape_is_where_the_sketch_plane_goes() {
        let mut b = Shape::new(Kind::Box, [0.0, 0.0, 15.0], Material::default());
        b.size = [30.0, 30.0, 30.0];
        assert!(near(top_of(&b), 30.0));
        let ball = Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material::default());
        assert!(near(top_of(&ball), 40.0));
        b.scale = [1.0, 1.0, 2.0];
        assert!(near(top_of(&b), 45.0));
        b.rot = [90.0, 0.0, 0.0];
        b.size = [10.0, 30.0, 10.0];
        b.scale = [1.0; 3];
        assert!(near(top_of(&b), 15.0 + 15.0), "turned: the long side is up");
        b.pos = [f32::NAN, 0.0, f32::NAN];
        assert_eq!(top_of(&b), 0.0);
    }
}
