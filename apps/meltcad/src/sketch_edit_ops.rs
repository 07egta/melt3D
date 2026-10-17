//! Conservative polygon editing operations for YuraCAD's existing closed 2D outlines.
//! The Shape format stores a *single simple polygon*, not a wire/edge graph. Operations which
//! would produce multiple contours, holes or an open wire are deliberately rejected.
use crate::model::MAX_OUTLINE;
use crate::sketch::{self, SketchError};

type Pt = [f32; 2];
const MIN_EDGE: f64 = 0.02; // shorter edges are unstable in the SDF outline
const PARALLEL_SIN: f64 = 1e-5;
const SAGITTA_MM: f64 = 0.03;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidOutline,
    InvalidValue,
    TooFewEdges,
    Parallel,
    CannotTrim,
    TooLarge,
    TooManyPoints,
    InvalidResult,
}

impl EditError {
    pub fn message(self) -> &'static str {
        match self {
            EditError::InvalidOutline => "Original outline is invalid. Fix crossings or tiny segments first",
            EditError::InvalidValue => "Distance and radius must be finite values greater than 0",
            EditError::TooFewEdges => "Cannot remove more edges",
            EditError::Parallel => "Nearly parallel lines have no safe intersection",
            EditError::CannotTrim => "This edge can't be trimmed; neighbors don't connect safely",
            EditError::TooLarge => "Distance or radius too large, or corners too short",
            EditError::TooManyPoints => "Keeping curve accuracy would exceed the point limit",
            EditError::InvalidResult => "Can't apply: would create self-intersections, flipped outline, or tiny segments",
        }
    }
}

fn sub(a: Pt, b: Pt) -> [f64; 2] {
    [f64::from(a[0]) - f64::from(b[0]), f64::from(a[1]) - f64::from(b[1])]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn length(a: [f64; 2]) -> f64 {
    dot(a, a).sqrt()
}
fn point(a: Pt, v: [f64; 2], t: f64) -> Pt {
    [(f64::from(a[0]) + v[0] * t) as f32, (f64::from(a[1]) + v[1] * t) as f32]
}
fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = length(v);
    (l > MIN_EDGE).then_some([v[0] / l, v[1] / l])
}
fn safe_original(p: &[Pt]) -> Result<(), EditError> {
    if p.len() < 3 || p.len() > MAX_OUTLINE || p.iter().any(|x| x.iter().any(|y| !y.is_finite())) {
        return Err(EditError::InvalidOutline);
    }
    if p.iter().enumerate().any(|(i, v)| length(sub(p[(i + 1) % p.len()], *v)) < MIN_EDGE) {
        return Err(EditError::InvalidOutline);
    }
    sketch::validate_profile(p).map(|_| ()).map_err(|_| EditError::InvalidOutline)
}
fn checked(original: &[Pt], candidate: Vec<Pt>) -> Result<Vec<Pt>, EditError> {
    if candidate.len() > MAX_OUTLINE {
        return Err(EditError::TooManyPoints);
    }
    if candidate.len() < 3 || candidate.iter().enumerate().any(|(i, p)| length(sub(candidate[(i + 1) % candidate.len()], *p)) < MIN_EDGE) {
        return Err(EditError::InvalidResult);
    }
    let old_area = sketch::signed_area(original);
    let new_area = sketch::signed_area(&candidate);
    if old_area == 0.0 || old_area.signum() != new_area.signum() {
        return Err(EditError::InvalidResult);
    }
    let mut clean = sketch::validate_profile(&candidate).map_err(|e| match e {
        SketchError::TooManyPoints => EditError::TooManyPoints,
        _ => EditError::InvalidResult,
    })?;
    // validate_profile canonicalizes to CCW; existing shape data may be clockwise.
    if old_area < 0.0 {
        clean.reverse();
    }
    Ok(clean)
}
fn line_intersection(a: Pt, av: [f64; 2], b: Pt, bv: [f64; 2]) -> Result<(Pt, f64, f64), EditError> {
    let scale = length(av) * length(bv);
    if scale < MIN_EDGE * MIN_EDGE {
        return Err(EditError::InvalidOutline);
    }
    let denom = cross(av, bv);
    if denom.abs() <= PARALLEL_SIN * scale {
        return Err(EditError::Parallel);
    }
    let ab = sub(b, a);
    let t = cross(ab, bv) / denom;
    let u = cross(ab, av) / denom;
    let p = point(a, av, t);
    if !p[0].is_finite() || !p[1].is_finite() {
        return Err(EditError::InvalidResult);
    }
    Ok((p, t, u))
}

/// Remove a short corner edge by extending its two neighboring edges to their intersection.
/// This is deliberately a *closed-outline trim*: free-form trimming needs a wire graph.
pub fn trim_edge(p: &[Pt], edge: usize) -> Result<Vec<Pt>, EditError> {
    safe_original(p)?;
    let n = p.len();
    if n < 4 {
        return Err(EditError::TooFewEdges);
    }
    if edge >= n {
        return Err(EditError::CannotTrim);
    }
    let (a, b, c, d) = (p[(edge + n - 1) % n], p[edge], p[(edge + 1) % n], p[(edge + 2) % n]);
    let (hit, t, u) = line_intersection(a, sub(b, a), c, sub(d, c))?;
    // Only remove a chamfer-like edge: incoming extends beyond b, outgoing backwards beyond c.
    if t < 1.0 - 1e-5 || u > 1e-5 {
        return Err(EditError::CannotTrim);
    }
    let cut_length = length(sub(c, b));
    if length(sub(hit, b)).max(length(sub(hit, c))) > cut_length * 8.0 {
        return Err(EditError::TooLarge);
    }
    let mut result = p.to_vec();
    result[edge] = hit;
    result.remove((edge + 1) % n);
    checked(p, result)
}

/// Parallel offset of a single simple closed polygon. Positive distance is *outward*
/// regardless of the original winding. Unsafe bevels, collapse and self-crossings are rejected.
pub fn offset_profile(p: &[Pt], distance: f32) -> Result<Vec<Pt>, EditError> {
    safe_original(p)?;
    if !distance.is_finite() || distance.abs() < 0.02 {
        return Err(EditError::InvalidValue);
    }
    let d = f64::from(distance);
    let n = p.len();
    let outward = if sketch::signed_area(p) > 0.0 { 1.0 } else { -1.0 };
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let prev = p[(i + n - 1) % n];
        let cur = p[i];
        let next = p[(i + 1) % n];
        let e0 = sub(cur, prev);
        let e1 = sub(next, cur);
        let v0 = unit(e0).ok_or(EditError::InvalidOutline)?;
        let v1 = unit(e1).ok_or(EditError::InvalidOutline)?;
        let normal = |v: [f64; 2]| [outward * v[1], -outward * v[0]];
        let n0 = normal(v0);
        let n1 = normal(v1);
        let s0 = point(cur, n0, d);
        let s1 = point(cur, n1, d);
        let hit = match line_intersection(s0, e0, s1, e1) {
            Ok((hit, _, _)) => hit,
            Err(EditError::Parallel) if dot(v0, v1) > 0.0 => {
                // A nearly straight corner needs no giant miter.
                let avg = unit([n0[0] + n1[0], n0[1] + n1[1]]).ok_or(EditError::Parallel)?;
                point(cur, avg, d)
            }
            Err(e) => return Err(e),
        };
        if length(sub(hit, cur)) > (d.abs() * 12.0).max(0.1) {
            return Err(EditError::TooLarge);
        }
        result.push(hit);
    }
    // An inward offset that has passed through itself can remain a simple polygon but reverse
    // each local edge; reject such collapsed/inverted contours.
    for i in 0..n {
        if dot(sub(result[(i + 1) % n], result[i]), sub(p[(i + 1) % n], p[i])) <= 0.0 {
            return Err(EditError::InvalidResult);
        }
    }
    checked(p, result)
}

/// Replace one polygon corner by a true circular tangent arc, adaptively tessellated to
/// at most 0.03 mm sagitta error. Does not silently lower precision to fit MAX_OUTLINE.
pub fn fillet_corner(p: &[Pt], corner: usize, radius: f32) -> Result<Vec<Pt>, EditError> {
    safe_original(p)?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err(EditError::InvalidValue);
    }
    if corner >= p.len() {
        return Err(EditError::InvalidOutline);
    }
    let n = p.len();
    let prev = p[(corner + n - 1) % n];
    let vtx = p[corner];
    let next = p[(corner + 1) % n];
    let to_prev = sub(prev, vtx);
    let to_next = sub(next, vtx);
    let a = unit(to_prev).ok_or(EditError::InvalidOutline)?;
    let b = unit(to_next).ok_or(EditError::InvalidOutline)?;
    let angle = dot(a, b).clamp(-1.0, 1.0).acos();
    if angle < 1e-3 || std::f64::consts::PI - angle < 1e-3 {
        return Err(EditError::Parallel);
    }
    let r = f64::from(radius);
    let tangent = r / (angle * 0.5).tan();
    if tangent < MIN_EDGE || tangent >= length(to_prev) - MIN_EDGE || tangent >= length(to_next) - MIN_EDGE {
        return Err(EditError::TooLarge);
    }
    let bisector = unit([a[0] + b[0], a[1] + b[1]]).ok_or(EditError::Parallel)?;
    let centre = point(vtx, bisector, r / (angle * 0.5).sin());
    let start = point(vtx, a, tangent);
    let end = point(vtx, b, tangent);
    let rs = sub(start, centre);
    let re = sub(end, centre);
    let sweep = cross(rs, re).atan2(dot(rs, re));
    if sweep.abs() < 1e-6 {
        return Err(EditError::Parallel);
    }
    // theta per segment limited by sagitta, rather than a fixed number of samples.
    let max_theta = 2.0 * (1.0 - (SAGITTA_MM / r).min(1.0)).acos();
    let steps = (sweep.abs() / max_theta.max(1e-4)).ceil().max(2.0) as usize;
    if n + steps > MAX_OUTLINE {
        return Err(EditError::TooManyPoints);
    }
    let theta0 = rs[1].atan2(rs[0]);
    let mut result = Vec::with_capacity(n + steps);
    for i in 0..n {
        if i == corner {
            for j in 0..=steps {
                if j == 0 {
                    result.push(start);
                } else if j == steps {
                    result.push(end);
                } else {
                    let theta = theta0 + sweep * j as f64 / steps as f64;
                    result.push(point(centre, [theta.cos(), theta.sin()], r));
                }
            }
        } else {
            result.push(p[i]);
        }
    }
    checked(p, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn square() -> Vec<Pt> {
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    }
    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }
    #[test]
    fn trim_removes_chamfer_and_preserves_winding() {
        let chamfer = vec![[0.0, 0.0], [8.0, 0.0], [10.0, 2.0], [10.0, 10.0], [0.0, 10.0]];
        let out = trim_edge(&chamfer, 1).unwrap();
        assert_eq!(out, square());
        let mut cw = chamfer;
        cw.reverse();
        let result = trim_edge(&cw, 2).unwrap();
        assert!(sketch::signed_area(&result) < 0.0);
        assert_eq!(result.len(), 4);
    }
    #[test]
    fn trim_refuses_parallel_and_non_chamfer_edges() {
        assert_eq!(trim_edge(&square(), 0), Err(EditError::Parallel));
        assert_eq!(trim_edge(&square()[..3], 0), Err(EditError::TooFewEdges));
    }
    #[test]
    fn offset_rectangles_outward_inward_and_clockwise() {
        let s = square();
        let out = offset_profile(&s, 2.0).unwrap();
        assert_eq!(out, vec![[-2.0, -2.0], [12.0, -2.0], [12.0, 12.0], [-2.0, 12.0]]);
        let inside = offset_profile(&s, -1.0).unwrap();
        assert!(near(inside[0][0], 1.0) && near(inside[0][1], 1.0));
        assert_eq!(offset_profile(&s, -6.0), Err(EditError::InvalidResult));
        let mut cw = s;
        cw.reverse();
        let cw_out = offset_profile(&cw, 2.0).unwrap();
        assert!(sketch::signed_area(&cw_out) < 0.0);
        assert!(near(sketch::signed_area(&cw_out).abs(), 196.0));
    }
    #[test]
    fn fillet_is_tangent_bounded_simple_and_reversible_winding() {
        let s = square();
        let f = fillet_corner(&s, 1, 2.0).unwrap();
        assert!(f.len() > s.len());
        assert!(near(f[1][0], 8.0) && near(f[1][1], 0.0));
        assert!(f.iter().any(|p| near(p[0], 10.0) && near(p[1], 2.0)));
        assert!(sketch::is_simple(&f));
        assert!(sketch::signed_area(&f) > 0.0);
        let mut cw = s;
        cw.reverse();
        let result = fillet_corner(&cw, 2, 2.0).unwrap();
        assert!(sketch::signed_area(&result) < 0.0);
    }
    #[test]
    fn pathological_inputs_fail_without_panic_or_precision_loss() {
        let s = square();
        assert_eq!(fillet_corner(&s, 1, 100.0), Err(EditError::TooLarge));
        assert_eq!(fillet_corner(&s, 1, f32::NAN), Err(EditError::InvalidValue));
        assert_eq!(offset_profile(&s, f32::INFINITY), Err(EditError::InvalidValue));
        assert!(trim_edge(&[[0.0, 0.0], [0.0, 0.0], [0.0, 10.0], [10.0, 10.0]], 0).is_err());
        let points: Vec<Pt> = (0..MAX_OUTLINE)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / MAX_OUTLINE as f32;
                [20.0 * a.cos(), 20.0 * a.sin()]
            })
            .collect();
        assert_eq!(fillet_corner(&points, 1, 1.0), Err(EditError::TooManyPoints));
    }
}
