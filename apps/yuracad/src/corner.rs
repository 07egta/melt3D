//! Chamfers (面取り): the other way `Shape::round` can treat an edge. Instead of growing a ball back
//! over the shrunk shape (a fillet) the corner is cut off with a flat plane, `round` mm along each
//! side of a right-angled corner (a 45-degree bevel).
//!
//! Every function is a max / min of 1-Lipschitz distances, so the sphere tracer can trust it (it
//! under-estimates a little outside the bevels, never over-estimates). The shader copy is
//! `shader_corner.wgsl` (same names with a `cn_` prefix); keep the two in step.

// `x.max(lo).min(hi)` is used on purpose: `clamp` panics when a bound is NaN or out of order.
#![allow(clippy::manual_clamp)]

/// sqrt(1/2): a bevel plane of a right-angled corner leaning at 45 degrees.
pub const S: f32 = std::f32::consts::FRAC_1_SQRT_2;

type V2 = [f32; 2];

/// The chamfered intersection of two things with signed distances `a` and `b` (negative inside):
/// the corner where they meet is cut off `c` along each side. The gradients of `a` and `b` must be
/// at right angles (a box face and another, the side and the end of a cylinder), or the bevel
/// would over-estimate the distance.
pub fn pair(a: f32, b: f32, c: f32) -> f32 {
    a.max(b).max((a + b + c) * S)
}

/// A box (half sizes `b`) with every edge cut off `c` along each side. `q` is the point.
pub fn cut_box(q: [f32; 3], b: [f32; 3], c: f32) -> f32 {
    let d = [q[0].abs() - b[0], q[1].abs() - b[1], q[2].abs() - b[2]];
    let bev = (d[0] + d[1] + c).max(d[0] + d[2] + c).max(d[1] + d[2] + c) * S;
    d[0].max(d[1]).max(d[2]).max(bev)
}

/// The bevel plane through the edge of two planes with signed distances `da`, `db` whose unit
/// normals have the dot product `nn`: it lies `c * S` inside the edge (the sharp corner).
pub fn bevel(da: f32, db: f32, nn: f32, c: f32) -> f32 {
    (da + db) / (2.0 + 2.0 * nn).max(0.05).sqrt() + c * S
}

fn unit(v: V2) -> V2 {
    let l = v[0].hypot(v[1]);
    if l < 1e-9 { [0.0, 0.0] } else { [v[0] / l, v[1] / l] }
}

/// Cuts the corner `cur` (between `prev` and `next`) off a flat shape with signed distance `d` at
/// the point `p`: `c` along each side, but at most half of either side. Only a convex corner is cut
/// (`orient` is +1 for a counter-clockwise outline, -1 for a clockwise one, and it also decides which
/// way the shape lies).
pub fn cut_vertex(d: f32, p: V2, prev: V2, cur: V2, next: V2, orient: f32, c: f32) -> f32 {
    let t1 = unit([cur[0] - prev[0], cur[1] - prev[1]]);
    let t2 = unit([next[0] - cur[0], next[1] - cur[1]]);
    if (t1[0] * t2[1] - t1[1] * t2[0]) * orient <= 1e-4 {
        return d;
    }
    let l1 = (cur[0] - prev[0]).hypot(cur[1] - prev[1]);
    let l2 = (next[0] - cur[0]).hypot(next[1] - cur[1]);
    let leg = c.min(l1 * 0.5).min(l2 * 0.5);
    if leg <= 1e-6 {
        return d;
    }
    let p1 = [cur[0] - t1[0] * leg, cur[1] - t1[1] * leg];
    let p2 = [cur[0] + t2[0] * leg, cur[1] + t2[1] * leg];
    let e = unit([p2[0] - p1[0], p2[1] - p1[1]]);
    // the side of the cut that is thrown away: outside for a counter-clockwise outline
    let n = [e[1] * orient, -e[0] * orient];
    let g = (p[0] - p1[0]) * n[0] + (p[1] - p1[1]) * n[1];
    // the piece cut off is everything beyond the cut line and within `leg` of the corner; taking
    // it out of the shape is a max with minus its distance (a safe under-estimate far inside)
    d.max(g.min(leg - (p[0] - cur[0]).hypot(p[1] - cur[1])))
}

/// Cuts every convex corner of the outline `v` off the shape with signed distance `d` at `p`. With
/// `axis` (a revolve profile) a side lying on the Z axis (x = 0) is no surface: the corner on the
/// axis is cut as if the other side were mirrored, which gives a flat cap square to the axis.
pub fn cut_polygon(mut d: f32, p: V2, v: &[V2], orient: f32, c: f32, axis: bool) -> f32 {
    let n = v.len();
    if n < 3 || c <= 0.0 {
        return d;
    }
    for i in 0..n {
        let (a, b, e) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
        let (mut prev, mut next) = (a, e);
        if axis {
            let (on_a, on_b, on_e) = (a[0] <= 1e-4, b[0] <= 1e-4, e[0] <= 1e-4);
            if on_b && on_a && on_e {
                continue;
            }
            if on_b && on_a {
                prev = [-e[0], e[1]];
            } else if on_b && on_e {
                next = [-a[0], a[1]];
            }
        }
        d = cut_vertex(d, p, prev, b, next, orient, c);
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_right_angled_corner_is_cut_c_along_each_side() {
        // the corner of a quarter plane x <= 0, y <= 0, cut 2 along each side
        let corner = |x: f32, y: f32| pair(x, y, 2.0);
        assert!((corner(-2.0, 0.0)).abs() < 1e-5); // where the cut meets the side
        assert!((corner(0.0, -2.0)).abs() < 1e-5);
        assert!(corner(0.0, 0.0) > 1.0); // the sharp tip is gone
        assert!(corner(-1.0, -1.0).abs() < 1e-5); // the middle of the cut
        assert!(corner(-5.0, -5.0) < -4.9); // deep inside nothing changed
    }

    #[test]
    fn cutting_a_square_corner_of_a_polygon() {
        let sq = [[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]];
        let d = |x: f32, y: f32| {
            let sharp = (x.abs() - 10.0).max(y.abs() - 10.0);
            cut_polygon(sharp, [x, y], &sq, 1.0, 3.0, false)
        };
        assert!(d(9.0, 9.0) > 0.0); // the tip is cut
        assert!((d(10.0, 7.0)).abs() < 1e-4); // the cut starts 3 down the side
        assert!((d(8.5, 8.5)).abs() < 1e-4); // the middle of the bevel
        assert!(d(0.0, 0.0) < -9.9);
        // all four corners are cut
        for (x, y) in [(9.5, -9.5), (-9.5, 9.5), (-9.5, -9.5)] {
            assert!(d(x, y) > 0.0);
        }
        // a clockwise outline works the same
        let cw: Vec<[f32; 2]> = sq.iter().rev().copied().collect();
        let dcw = cut_polygon((9.0f32.abs() - 10.0).max(9.0 - 10.0), [9.0, 9.0], &cw, -1.0, 3.0, false);
        assert!(dcw > 0.0);
    }

    #[test]
    fn a_concave_corner_and_far_points_are_left_alone() {
        // an L shape: only the convex corners are cut, never the inside corner at (0, 0)
        let l = [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0], [10.0, 20.0], [0.0, 20.0]];
        let sharp = crate::sdf::polygon(&l, [5.0, 10.0]);
        assert!((cut_polygon(sharp, [5.0, 10.0], &l, 1.0, 3.0, false) - sharp).abs() < 1e-6);
        let inner = crate::sdf::polygon(&l, [10.5, 10.5]);
        let cut = cut_polygon(inner, [10.5, 10.5], &l, 1.0, 3.0, false);
        assert!((cut - inner).abs() < 1e-6, "{cut} vs {inner}");
        // the reflex corner at (10, 10) keeps its notch
        let near = crate::sdf::polygon(&l, [12.0, 12.0]);
        assert!((cut_polygon(near, [12.0, 12.0], &l, 1.0, 3.0, false) - near).abs() < 1e-6);
        // the convex corner at (20, 0) is cut
        assert!(cut_polygon(crate::sdf::polygon(&l, [19.5, 0.5]), [19.5, 0.5], &l, 1.0, 3.0, false) > 0.0);
    }

    #[test]
    fn the_axis_corner_of_a_profile_gets_a_flat_cap() {
        // a profile of a cone: (0,0) (10,0) (0,20) with the Z axis as its left side
        let tri = [[0.0, 0.0], [10.0, 0.0], [0.0, 20.0]];
        let sharp = |x: f32, z: f32| crate::cad::polygon_off_axis(&tri, [x, z]);
        let cut = |x: f32, z: f32| cut_polygon(sharp(x, z), [x, z], &tri, 1.0, 3.0, true);
        assert!(cut(0.0, 20.0) > 2.0); // the tip is flat now
        assert!(cut(0.0, 10.0) < -4.0); // the middle of the axis is not touched
        assert!(cut(9.9, 0.1) > 0.3); // the base rim is cut too
    }

    #[test]
    fn hostile_numbers_stay_finite_or_harmless() {
        let sq = [[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]];
        for c in [0.0, -3.0, 1e9, f32::NAN] {
            let v = cut_polygon(1.0, [1.0, 2.0], &sq, 1.0, c, false);
            assert!(v.is_finite() || c.is_nan());
        }
        assert!(cut_polygon(1.0, [1.0, 2.0], &[], 1.0, 3.0, false).is_finite());
        assert!(cut_box([1.0, 2.0, 3.0], [5.0, 5.0, 5.0], 1e9).is_finite());
    }

    // ---- whole shapes (sdf.rs), rounded and chamfered ----

    use crate::model::{Corner, Kind, Material, Shape};
    use crate::sdf::Prepared;

    fn shape(kind: Kind, size: [f32; 3], round: f32, corner: Corner) -> Prepared {
        let mut s = Shape::new(kind, [0.0; 3], Material::default());
        s.size = size;
        s.round = round;
        s.corner = corner;
        Prepared::new(&s)
    }

    fn near(a: f32, b: f32) {
        assert!((a - b).abs() < 2e-3 * (1.0 + b.abs()), "{a} != {b}");
    }

    /// The distance of the corner point of a right angle to a fillet / a bevel of size `r`.
    const FILLET: f32 = 0.414_213_57; // sqrt(2) - 1
    const BEVEL: f32 = S;

    #[test]
    fn a_rounded_and_a_chamfered_box_edge_differ() {
        let b = [20.0, 20.0, 20.0];
        let round = shape(Kind::Box, b, 4.0, Corner::Round);
        let cham = shape(Kind::Box, b, 4.0, Corner::Chamfer);
        near(round.local_distance_for_test([10.0, 10.0, 0.0]), 4.0 * FILLET);
        near(cham.local_distance_for_test([10.0, 10.0, 0.0]), 4.0 * BEVEL);
        // the flat faces are untouched by both
        near(round.local_distance_for_test([10.0, 0.0, 0.0]), 0.0);
        near(cham.local_distance_for_test([10.0, 0.0, 0.0]), 0.0);
        near(cham.local_distance_for_test([0.0, 0.0, 0.0]), -10.0);
        // the vertex: a ball of radius 4 about (6, 6, 6) against the three bevels
        near(round.local_distance_for_test([10.0, 10.0, 10.0]), 3.0f32.sqrt() * 4.0 - 4.0);
        near(cham.local_distance_for_test([10.0, 10.0, 10.0]), 4.0 * BEVEL);
        // the bevel starts 4 from the corner along each side
        near(cham.local_distance_for_test([10.0, 6.0, 0.0]), 0.0);
        near(cham.local_distance_for_test([6.0, 10.0, 0.0]), 0.0);
    }

    #[test]
    fn rounded_and_chamfered_cylinder_cone_and_wedge() {
        let cyl = [10.0, 20.0, 1.0];
        let r = shape(Kind::Cylinder, cyl, 3.0, Corner::Round);
        let c = shape(Kind::Cylinder, cyl, 3.0, Corner::Chamfer);
        near(r.local_distance_for_test([10.0, 0.0, 10.0]), 3.0 * FILLET);
        near(c.local_distance_for_test([10.0, 0.0, 10.0]), 3.0 * BEVEL);
        near(c.local_distance_for_test([7.0, 0.0, 10.0]), 0.0);

        // a cone, base radius 10, height 20: the tip is blunted by r * (1 / sin(half angle) - 1)
        let cone = [10.0, 20.0, 1.0];
        let r = shape(Kind::Cone, cone, 2.0, Corner::Round);
        let half = (10.0f32 / 20.0).atan();
        near(r.local_distance_for_test([0.0, 0.0, 10.0]), 2.0 * (1.0 / half.sin() - 1.0));
        let c = shape(Kind::Cone, cone, 2.0, Corner::Chamfer);
        assert!(c.local_distance_for_test([0.0, 0.0, 10.0]) > 0.5); // the tip is cut flat
        assert!(c.local_distance_for_test([10.0, 0.0, -10.0]) > 0.3); // and so is the base rim
        near(c.local_distance_for_test([0.0, 0.0, -10.0]), 0.0); // the base stays flat
        near(c.local_distance_for_test([0.0, 0.0, 0.0]), r.local_distance_for_test([0.0, 0.0, 0.0]));

        // a wedge: the square corner at the foot of its tall side
        let w = [30.0, 30.0, 30.0];
        let r = shape(Kind::Wedge, w, 3.0, Corner::Round);
        let c = shape(Kind::Wedge, w, 3.0, Corner::Chamfer);
        near(r.local_distance_for_test([-15.0, 0.0, -15.0]), 3.0 * FILLET);
        near(c.local_distance_for_test([-15.0, 0.0, -15.0]), 3.0 * BEVEL);
        // the front face and the slope stay put
        near(c.local_distance_for_test([-5.0, 15.0, -5.0]), 0.0);
        near(r.local_distance_for_test([0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn prisms_stars_and_hemispheres_can_be_chamfered() {
        let p = shape(Kind::PrismN, [22.0, 40.0, 1.0], 4.0, Corner::Chamfer);
        let mut sq = Shape::new(Kind::PrismN, [0.0; 3], Material::default());
        sq.size = [22.0, 40.0, 1.0];
        sq.params = [4.0, 0.0, 0.0, 0.0];
        sq.round = 4.0;
        sq.corner = Corner::Chamfer;
        let sq = Prepared::new(&sq);
        // the vertical edge at (22, 0): a square's corner is a right angle
        near(sq.local_distance_for_test([22.0, 0.0, 0.0]), 4.0 * BEVEL);
        near(sq.local_distance_for_test([0.0, 0.0, 20.0]), 0.0); // the flat top stays
        near(sq.local_distance_for_test([0.0, 0.0, 0.0]), -22.0 * S);
        assert!(p.local_distance_for_test([22.0, 0.0, 0.0]) > 0.5); // a hexagon's corner is cut too
        let mut st = Shape::new(Kind::Star, [0.0; 3], Material::default());
        st.size = [24.0, 14.0, 1.0];
        st.params = [5.0, 0.45, 0.0, 0.0];
        st.round = 2.0;
        st.corner = Corner::Chamfer;
        let st = Prepared::new(&st);
        assert!(st.local_distance_for_test([24.0, 0.0, 0.0]) > 0.5); // the tip is cut off
        near(st.local_distance_for_test([0.0, 0.0, 17.0]), 10.0);
        let h = shape(Kind::Hemi, [22.0, 1.0, 1.0], 3.0, Corner::Chamfer);
        near(h.local_distance_for_test([0.0, 0.0, 11.0]), 0.0); // the dome top is smooth
        assert!(h.local_distance_for_test([22.0, 0.0, -11.0]) > 1.0); // the rim is cut
        near(h.local_distance_for_test([0.0, 0.0, -11.0]), 0.0);
    }

    #[test]
    fn extrude_corners_round_in_the_plane_too() {
        // the default extrude is a square-ish outline: its corners are rounded now
        let sharp = shape(Kind::Extrude, [20.0, 1.0, 1.0], 0.0, Corner::Round);
        let r = shape(Kind::Extrude, [20.0, 1.0, 1.0], 4.0, Corner::Round);
        let c = shape(Kind::Extrude, [20.0, 1.0, 1.0], 4.0, Corner::Chamfer);
        let corner = {
            let o = &sharp.outline;
            let m = o.iter().fold([0.0f32; 2], |m, p| [m[0].max(p[0].abs()), m[1].max(p[1].abs())]);
            [m[0], m[1]]
        };
        let at = |x: &Prepared| x.local_distance_for_test([corner[0], corner[1], 0.0]);
        assert!(at(&sharp).abs() < 1e-3);
        assert!(at(&r) > 0.5, "rounded corner {}", at(&r));
        assert!(at(&c) > 0.5, "chamfered corner {}", at(&c));
        assert!((at(&r) - at(&c)).abs() > 0.2);
    }

    #[test]
    fn rounding_and_chamfering_never_grow_a_shape_and_stay_safe_to_march() {
        let mut rng = 12345u64;
        let mut next = move || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((rng >> 40) as f32) / (1u64 << 24) as f32
        };
        for &kind in &Kind::ALL {
            if !kind.can_round() {
                continue;
            }
            let base = Shape::new(kind, [0.0; 3], Material::default());
            let sharp = Prepared::new(&base);
            for corner in Corner::ALL {
                for round in [2.5f32, 9.0, 400.0] {
                    let mut s = base.clone();
                    s.round = round;
                    s.corner = corner;
                    let p = Prepared::new(&s);
                    let reach = Shape::new(kind, [0.0; 3], Material::default()).local_half().iter().fold(10.0f32, |a, b| a.max(*b)) * 1.5;
                    // the centre is still inside (a shape never rounds away completely)
                    for k in 0..400 {
                        let a = [(next() - 0.5) * 2.0 * reach, (next() - 0.5) * 2.0 * reach, (next() - 0.5) * 2.0 * reach];
                        let b = [(next() - 0.5) * 2.0 * reach, (next() - 0.5) * 2.0 * reach, (next() - 0.5) * 2.0 * reach];
                        let (da, db) = (p.local_distance_for_test(a), p.local_distance_for_test(b));
                        assert!(da.is_finite() && db.is_finite(), "{kind:?} {corner:?} {round}");
                        let dist = crate::sdf::len(crate::sdf::sub(a, b));
                        assert!((da - db).abs() <= dist * 1.02 + 1e-2, "{kind:?} {corner:?} {round}: {da} vs {db} over {dist} (point {k})");
                        // outside the sharp shape is outside the rounded one
                        let ds = sharp.local_distance_for_test(a);
                        if ds > 0.05 {
                            assert!(da > -0.05, "{kind:?} {corner:?} {round}: outside the sharp shape ({ds}) but inside ({da})");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hostile_rounds_never_panic() {
        let weird = [0.0, -5.0, 1e-9, 1e12, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        for &kind in &Kind::ALL {
            for corner in Corner::ALL {
                for &r in &weird {
                    for &sz in &[1.0, f32::NAN, 1e-9] {
                        let mut s = Shape::new(kind, [0.0; 3], Material::default());
                        s.size = [sz, sz, sz];
                        s.round = r;
                        s.corner = corner;
                        let p = Prepared::new(&s);
                        for q in [[0.0; 3], [1.0, 2.0, 3.0], [r, sz, r]] {
                            let _ = p.local_distance_for_test(q);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_corner_style_survives_files_old_and_new() {
        let mut s = Shape::new(Kind::Box, [0.0; 3], Material::default());
        s.round = 3.0;
        s.corner = Corner::Chamfer;
        let json = serde_json::to_string(&s).unwrap_or_default();
        let back: Shape = serde_json::from_str(&json).unwrap_or_else(|_| Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        assert_eq!(back, s);
        // a file from before has no `corner`: it is round
        let old = json.replace(",\"corner\":\"Chamfer\"", "");
        assert!(!old.contains("corner"));
        let older: Shape = serde_json::from_str(&old).unwrap_or_else(|_| Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        assert_eq!(older.kind, Kind::Box);
        assert_eq!(older.corner, Corner::Round);
        // a name from the future is round too
        let odd: Shape = serde_json::from_str(&json.replace("Chamfer", "Bullnose")).unwrap_or_else(|_| Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        assert_eq!(odd.corner, Corner::Round);
        assert_eq!(odd.kind, Kind::Box);
    }

    /// `YT_OUT=/tmp/showcase.yura cargo test -p yuracad dump_round_showcase -- --ignored` writes a scene
    /// with every kind that can round, sharp / rounded / chamfered in three rows (for looking at).
    #[test]
    #[ignore]
    fn dump_round_showcase() {
        let out = std::env::var("YT_OUT").unwrap_or_else(|_| "/tmp/showcase.yura".into());
        let mut sc = crate::model::Scene::default();
        sc.groups[0].shapes.clear();
        sc.groups[0].blend = 0.0;
        let from: usize = std::env::var("YT_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let count: usize = std::env::var("YT_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
        let kinds: Vec<Kind> = Kind::ALL.iter().copied().filter(|k| k.can_round()).skip(from).take(count).collect();
        for (col, &k) in kinds.iter().enumerate() {
            for (row, (round, corner)) in [(0.0, Corner::Round), (5.0, Corner::Round), (5.0, Corner::Chamfer)].into_iter().enumerate() {
                let mut sh = Shape::new(k, [col as f32 * 48.0, row as f32 * 48.0, 0.0], Material::default());
                sh.blend = 0.0;
                sh.round = round;
                sh.corner = corner;
                sc.groups[0].shapes.push(sh);
            }
        }
        let _ = std::fs::write(out, sc.to_json());
    }
}
