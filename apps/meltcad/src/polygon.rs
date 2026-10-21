//! Cutting a flat polygon face (an OBJ `f` line or a PLY face with more than three corners) into
//! triangles that stay inside its outline, so an L, a U or a star keeps its shape.
//! (A simple fan from the first corner is only right for convex faces.)

/// The most corners one face is cut with ear clipping; longer faces fall back to a fan.
const MAX_EAR_CORNERS: usize = 200;

/// Triangles for the face whose corners are `idx` (indices into `points`), as index triples.
/// A face with a corner that is not in `points` is fanned, and the caller's own check reports it.
pub fn triangulate(points: &[[f32; 3]], idx: &[u32]) -> Vec<[u32; 3]> {
    let n = idx.len();
    if n < 3 {
        return Vec::new();
    }
    let fan = || (2..n).filter_map(|k| Some([*idx.first()?, *idx.get(k - 1)?, *idx.get(k)?])).collect::<Vec<_>>();
    if n == 3 || n > MAX_EAR_CORNERS {
        return fan();
    }
    let Some(pts) = idx.iter().map(|&i| points.get(i as usize).copied()).collect::<Option<Vec<[f32; 3]>>>() else { return fan() };
    // the face's normal (Newell), then the two axes that stay once its strongest axis is dropped
    let mut nrm = [0.0f64; 3];
    for i in 0..n {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { return fan() };
        nrm[0] += f64::from(a[1] - b[1]) * f64::from(a[2] + b[2]);
        nrm[1] += f64::from(a[2] - b[2]) * f64::from(a[0] + b[0]);
        nrm[2] += f64::from(a[0] - b[0]) * f64::from(a[1] + b[1]);
    }
    let axis = if nrm[0].abs() >= nrm[1].abs() && nrm[0].abs() >= nrm[2].abs() {
        0
    } else if nrm[1].abs() >= nrm[2].abs() {
        1
    } else {
        2
    };
    if nrm[axis].abs() < 1e-12 || !nrm[axis].is_finite() {
        return fan();
    }
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    // flat coordinates, turned so the outline runs counter-clockwise
    let sign = if nrm[axis] >= 0.0 { 1.0 } else { -1.0 };
    let flat: Vec<[f64; 2]> = pts.iter().map(|p| [f64::from(p[u]) * sign, f64::from(p[v])]).collect();
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0usize;
    while left.len() > 3 && guard < n * n + 8 {
        guard += 1;
        let m = left.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
            let (a, b, c) = (flat[ia], flat[ib], flat[ic]);
            if cross(a, b, c) <= 1e-12 {
                continue; // a reflex (or flat) corner is not an ear
            }
            let inside = left.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let p = flat[j];
                if p == a || p == b || p == c {
                    return false;
                }
                cross(a, b, p) >= -1e-12 && cross(b, c, p) >= -1e-12 && cross(c, a, p) >= -1e-12
            });
            if !inside {
                out.push([idx[ia], idx[ib], idx[ic]]);
                left.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // a self-touching or degenerate outline: cut off the first corner so it still ends
            let m = left.len();
            out.push([idx[left[m - 1]], idx[left[0]], idx[left[1]]]);
            left.remove(0);
        }
    }
    if let [a, b, c] = left[..] {
        out.push([idx[a], idx[b], idx[c]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[[f32; 3]], tris: &[[u32; 3]]) -> f32 {
        tris.iter()
            .map(|t| {
                let p = |i: u32| points[i as usize];
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                (((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.0).abs()
            })
            .sum()
    }

    #[test]
    fn a_u_shape_is_cut_inside_its_outline() {
        // the U of the report: 4 x 4 with a 2 x 3 notch from the top: area 16 - 6 = 10
        let pts = [[0., 0., 0.], [4., 0., 0.], [4., 4., 0.], [3., 4., 0.], [3., 1., 0.], [1., 1., 0.], [1., 4., 0.], [0., 4., 0.]];
        let idx: Vec<u32> = (0..8).collect();
        let tris = triangulate(&pts, &idx);
        assert_eq!(tris.len(), 6);
        assert!((area(&pts, &tris) - 10.0).abs() < 1e-4, "{}", area(&pts, &tris));
        // the same face listed clockwise, and standing on another plane, gives the same area
        let rev: Vec<u32> = (0..8).rev().collect();
        assert!((area(&pts, &triangulate(&pts, &rev)) - 10.0).abs() < 1e-4);
        let up: Vec<[f32; 3]> = pts.iter().map(|p| [p[0], 0.0, p[1]]).collect();
        let t2 = triangulate(&up, &idx);
        let flat: Vec<[f32; 3]> = up.iter().map(|p| [p[0], p[2], 0.0]).collect();
        assert!((area(&flat, &t2) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn convex_faces_and_bad_input_still_work() {
        let sq = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
        let t = triangulate(&sq, &[0, 1, 2, 3]);
        assert_eq!(t.len(), 2);
        assert!((area(&sq, &t) - 1.0).abs() < 1e-5);
        assert!(triangulate(&sq, &[0, 1]).is_empty());
        assert_eq!(triangulate(&sq, &[0, 1, 99, 3]).len(), 2, "unknown corner: fanned, reported later");
        // all in a line (no area), repeated corners and NaN never panic
        let line = [[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]];
        assert_eq!(triangulate(&line, &[0, 1, 2, 3]).len(), 2);
        let nan = [[f32::NAN; 3]; 5];
        assert_eq!(triangulate(&nan, &[0, 1, 2, 3, 4]).len(), 3);
        assert_eq!(triangulate(&sq, &[0, 0, 0, 0]).len(), 2);
    }
}
