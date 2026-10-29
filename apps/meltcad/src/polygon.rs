//! Cutting a flat polygon face (an OBJ `f` line or a PLY face with more than three corners) into
//! triangles that stay inside its outline, so an L, a U or a star keeps its shape.
//! (A simple fan from the first corner is only right for convex faces.)

/// The most point tests one face may cost; past it the rest of the face is cut corner by corner
/// so a hostile file still ends quickly (a real face of 100 000 corners stays well inside it).
const WORK_BUDGET: u64 = 400_000_000;

/// Triangles for the face whose corners are `idx` (indices into `points`), as index triples.
/// A face with a corner that is not in `points` is fanned, and the caller's own check reports it.
pub fn triangulate(points: &[[f32; 3]], idx: &[u32]) -> Vec<[u32; 3]> {
    let n = idx.len();
    if n < 3 {
        return Vec::new();
    }
    let fan = || (2..n).filter_map(|k| Some([*idx.first()?, *idx.get(k - 1)?, *idx.get(k)?])).collect::<Vec<_>>();
    if n == 3 {
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
    // the outline as a ring (prev/next) so cutting an ear is O(1), and only reflex corners are
    // tested against a candidate ear (a corner inside an ear is always a reflex one)
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let corner_reflex = |i: usize, prev: &[usize], next: &[usize]| -> bool {
        match (prev.get(i), next.get(i)) {
            (Some(&a), Some(&c)) => match (flat.get(a), flat.get(i), flat.get(c)) {
                (Some(&pa), Some(&pb), Some(&pc)) => cross(pa, pb, pc) <= 1e-12,
                _ => true,
            },
            _ => true,
        }
    };
    let mut reflex: Vec<bool> = (0..n).map(|i| corner_reflex(i, &prev, &next)).collect();
    let mut reflex_list: Vec<usize> = (0..n).filter(|&i| reflex.get(i).copied().unwrap_or(false)).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut left = n;
    let mut cur = 0usize;
    let mut misses = 0usize; // corners looked at since the last cut
    // a hard cap on point tests so a huge or hostile face always ends quickly
    let mut budget: u64 = WORK_BUDGET;
    while left > 3 {
        let (Some(&ia), Some(&ic)) = (prev.get(cur), next.get(cur)) else { break };
        let ib = cur;
        let mut is_ear = false;
        if !reflex.get(ib).copied().unwrap_or(true)
            && let (Some(&a), Some(&b), Some(&c)) = (flat.get(ia), flat.get(ib), flat.get(ic))
        {
            {
                is_ear = true;
                reflex_list.retain(|&j| alive.get(j).copied().unwrap_or(false) && reflex.get(j).copied().unwrap_or(false));
                for &j in &reflex_list {
                    budget = budget.saturating_sub(1);
                    if j == ia || j == ib || j == ic {
                        continue;
                    }
                    let Some(&p) = flat.get(j) else { continue };
                    if p == a || p == b || p == c {
                        continue;
                    }
                    if cross(a, b, p) >= -1e-12 && cross(b, c, p) >= -1e-12 && cross(c, a, p) >= -1e-12 {
                        is_ear = false;
                        break;
                    }
                }
            }
        }
        budget = budget.saturating_sub(1);
        // no ear found after a full lap (self-touching or degenerate outline) or out of budget:
        // cut this corner anyway so it still ends
        let force = misses > left || budget == 0;
        if is_ear || force {
            out.push([idx[ia], idx[ib], idx[ic]]);
            if let Some(x) = next.get_mut(ia) {
                *x = ic;
            }
            if let Some(x) = prev.get_mut(ic) {
                *x = ia;
            }
            if let Some(x) = alive.get_mut(ib) {
                *x = false;
            }
            left -= 1;
            misses = 0;
            for j in [ia, ic] {
                let r = corner_reflex(j, &prev, &next);
                if let Some(x) = reflex.get_mut(j) {
                    *x = r;
                }
            }
            cur = ia;
        } else {
            misses += 1;
            cur = ic;
        }
    }
    if left == 3
        && let (Some(&a), Some(&c)) = (prev.get(cur), next.get(cur))
    {
        out.push([idx[a], idx[cur], idx[c]]);
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

    /// #121: a U face with more than 200 corners is still cut inside its outline (no fan).
    #[test]
    fn a_long_u_face_is_not_fanned() {
        let outline = [[0., 0.], [4., 0.], [4., 4.], [3., 4.], [3., 1.], [1., 1.], [1., 4.], [0., 4.]];
        let mut pts = Vec::new();
        for k in 0..8 {
            let (a, b): ([f32; 2], [f32; 2]) = (outline[k], outline[(k + 1) % 8]);
            for s in 0..26 {
                let t = s as f32 / 26.0;
                pts.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, 0.0]);
            }
        }
        assert_eq!(pts.len(), 208);
        let idx: Vec<u32> = (0..208).collect();
        let tris = triangulate(&pts, &idx);
        assert_eq!(tris.len(), 206);
        assert!((area(&pts, &tris) - 10.0).abs() < 1e-3, "{}", area(&pts, &tris));
        // a big convex face (a circle of 20 000 corners) is cut quickly and fully
        let n = 20_000u32;
        let ring: Vec<[f32; 3]> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                [a.cos(), a.sin(), 0.0]
            })
            .collect();
        let idx: Vec<u32> = (0..n).collect();
        let tris = triangulate(&ring, &idx);
        assert_eq!(tris.len(), n as usize - 2);
        assert!((area(&ring, &tris) - std::f32::consts::PI).abs() < 1e-2);
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
