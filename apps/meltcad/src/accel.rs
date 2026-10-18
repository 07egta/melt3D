//! Speed-up structure shared by the shader's data (gpu.rs) and the CPU field (sdf.rs): a group's
//! shapes are cut into blocks of `BLOCK` neighbours in the list, and every block and every group
//! gets a bounding sphere. A ray step that is far from a block never touches its shapes, and one
//! that is far from a whole group never touches the group. The tests prove the skips never change
//! the answer.

use crate::model::{BlendMode, Melt, Op, Shape};

/// Shapes per block.
pub const BLOCK: usize = 8;

/// A sphere (centre, radius).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bound {
    pub c: [f32; 3],
    pub r: f32,
}

impl Bound {
    pub const NONE: Bound = Bound { c: [0.0; 3], r: 0.0 };
    /// How far `p` is from the sphere (a lower bound of the distance to whatever it holds).
    pub fn lower(&self, p: [f32; 3]) -> f32 {
        let d = [p[0] - self.c[0], p[1] - self.c[1], p[2] - self.c[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - self.r
    }
}

/// A sphere around the given spheres: the middle of their boxes, then the farthest edge.
pub fn enclose(items: &[Bound]) -> Bound {
    if items.is_empty() {
        return Bound::NONE;
    }
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for b in items {
        for i in 0..3 {
            lo[i] = lo[i].min(b.c[i] - b.r);
            hi[i] = hi[i].max(b.c[i] + b.r);
        }
    }
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let r = items.iter().map(|b| b.lower(c) + 2.0 * b.r).fold(0.0_f32, f32::max);
    // `lower` is distance minus radius, so distance + radius = lower + 2 r
    if c.iter().all(|v| v.is_finite()) && r.is_finite() { Bound { c, r: r.min(1.0e5) } } else { Bound { c: [0.0; 3], r: 1.0e5 } }
}

/// The sphere of one shape: centred on its position.
pub fn shape_bound(s: &Shape) -> Bound {
    Bound { c: s.pos, r: crate::perf::shape_radius(s) }
}

/// How far a blend reaches in units of its size (the soft minimum is exact beyond it).
pub fn reach_mult(mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Smooth => 1.0,
        BlendMode::Round => 1.3,
        BlendMode::Gooey => 6.0,
    }
}

/// The most a soft union of size `k` can pull the distance below the smaller of the two.
pub fn pull(mode: BlendMode, k: f32) -> f32 {
    match mode {
        BlendMode::Smooth => 0.25 * k,
        BlendMode::Round => 0.55 * k,
        BlendMode::Gooey => k,
    }
}

/// How far from `a` a value `b` must be for a blend of size `k` to return `a` unchanged. It is the
/// same arithmetic as `far_enough()` in shader.wgsl.
pub fn far_enough(a: f32, k: f32, mode: BlendMode) -> f32 {
    let rm = reach_mult(mode);
    let need = rm * k + a + 0.02;
    if mode == BlendMode::Round { need.max(rm * k + 0.02) } else { need }
}

/// How far a stain must be from the surface to leave it alone (shader: `0.5 * max(k * 0.35, 0.3) + 0.02`).
pub fn paint_far(k: f32) -> f32 {
    0.5 * (k * 0.35).max(0.3) + 0.02
}

/// When a whole block can be skipped: it is farther than this (`block_far()` in shader.wgsl). It
/// holds for every add, carve and stain in the block, whatever the sign of the running value `g`.
pub fn block_far(g: f32, kmax: f32, mode: BlendMode) -> f32 {
    (reach_mult(mode) * kmax + g.abs() + 0.02).max(paint_far(kmax))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub bound: Bound,
    /// The largest blend size (before the melt amount) in the block.
    pub kmax: f32,
    /// False when the block holds an overlap shape: that one changes the result even far away.
    pub skippable: bool,
    /// Index of its first shape in the group's visible list, and how many.
    pub first: usize,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupInfo {
    /// A sphere that holds every part of the group that can lower the distance anywhere (its adds
    /// and overlaps, and the pull of their blends).
    pub bound: Bound,
    pub blocks: Vec<Block>,
}

/// The blocks and the sphere of a group whose visible shapes are `shapes`.
pub fn group_info(shapes: &[&Shape], melt: &Melt) -> GroupInfo {
    let bounds: Vec<Bound> = shapes.iter().map(|s| shape_bound(s)).collect();
    group_info_with_bounds(shapes, &bounds, melt)
}

/// Build the same acceleration records from already measured shape bounds. A curve's
/// bound resamples its path, so fields and GPU packs retain that work alongside their
/// prepared geometry. The bounds must describe these shapes in this exact order.
pub fn group_info_with_bounds(shapes: &[&Shape], bounds: &[Bound], melt: &Melt) -> GroupInfo {
    if bounds.len() != shapes.len() {
        // A caller without a complete matching list still gets correct bounds.
        return group_info(shapes, melt);
    }
    let mut blocks = Vec::new();
    let mut first = 0;
    for (chunk, spheres) in shapes.chunks(BLOCK).zip(bounds.chunks(BLOCK)) {
        blocks.push(Block {
            bound: enclose(spheres),
            kmax: chunk.iter().map(|s| clean(s.blend)).fold(0.0_f32, f32::max),
            skippable: chunk.iter().all(|s| s.op != Op::Intersect),
            first,
            count: chunk.len(),
        });
        first += chunk.len();
    }
    // what can lower the distance: adds and overlaps (a carve or a stain never does)
    let spheres: Vec<Bound> = shapes.iter().zip(bounds).filter(|(s, _)| matches!(s.op, Op::Add | Op::Intersect)).map(|(_, b)| *b).collect();
    let mut bound = enclose(&spheres);
    let total_pull: f32 = shapes.iter().filter(|s| s.op == Op::Add).map(|s| pull(melt.mode, clean(s.blend) * melt.amount.max(0.0))).sum();
    bound.r = (bound.r + total_pull + 0.5).min(1.0e5);
    GroupInfo { bound, blocks }
}

fn clean(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0e4) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material};

    /// The pre-reuse algorithm measured each block and the lowering set separately.
    /// Retain it as a reference: cached bounds must preserve every floating-point bit.
    fn reference_group_info(shapes: &[&Shape], melt: &Melt) -> GroupInfo {
        let mut blocks = Vec::new();
        let mut first = 0;
        for chunk in shapes.chunks(BLOCK) {
            let spheres: Vec<Bound> = chunk.iter().map(|s| shape_bound(s)).collect();
            blocks.push(Block {
                bound: enclose(&spheres),
                kmax: chunk.iter().map(|s| clean(s.blend)).fold(0.0_f32, f32::max),
                skippable: chunk.iter().all(|s| s.op != Op::Intersect),
                first,
                count: chunk.len(),
            });
            first += chunk.len();
        }
        let lowering: Vec<&&Shape> = shapes.iter().filter(|s| matches!(s.op, Op::Add | Op::Intersect)).collect();
        let spheres: Vec<Bound> = lowering.iter().map(|s| shape_bound(s)).collect();
        let mut bound = enclose(&spheres);
        let total_pull: f32 = lowering.iter().filter(|s| s.op == Op::Add).map(|s| pull(melt.mode, clean(s.blend) * melt.amount.max(0.0))).sum();
        bound.r = (bound.r + total_pull + 0.5).min(1.0e5);
        GroupInfo { bound, blocks }
    }

    #[test]
    fn retained_shape_bounds_match_the_original_acceleration_records_bit_for_bit() {
        let shapes: Vec<Shape> = Kind::ALL
            .into_iter()
            .chain([Kind::Mesh])
            .enumerate()
            .map(|(i, kind)| {
                let mut shape = Shape::new(kind, [i as f32 * 13.7, -0.0, i as f32 * -2.5], Material::default());
                shape.scale = [0.01 + i as f32 * 0.07, 1.3, 2.1];
                shape.rot = [17.0, -35.0, 83.0];
                shape.op = [Op::Add, Op::Subtract, Op::Intersect, Op::Paint][i % 4];
                shape.blend = i as f32 * 0.625;
                shape.shell = 0.8;
                shape
            })
            .collect();
        let refs: Vec<&Shape> = shapes.iter().collect();
        let bounds: Vec<Bound> = refs.iter().map(|shape| shape_bound(shape)).collect();
        let bits = |bound: &Bound| (bound.c.map(f32::to_bits), bound.r.to_bits());
        let records = |info: &GroupInfo| {
            (
                bits(&info.bound),
                info.blocks
                    .iter()
                    .map(|block| (bits(&block.bound), block.kmax.to_bits(), block.skippable, block.first, block.count))
                    .collect::<Vec<_>>(),
            )
        };
        for count in [0, 1, BLOCK - 1, BLOCK, BLOCK + 1, shapes.len()] {
            for mode in BlendMode::ALL {
                for amount in [-0.0, 0.25, 1.0, 3.0] {
                    let melt = Melt { mode, amount, ..Melt::default() };
                    let expected = reference_group_info(&refs[..count], &melt);
                    assert_eq!(records(&group_info_with_bounds(&refs[..count], &bounds[..count], &melt)), records(&expected));
                    assert_eq!(records(&group_info(&refs[..count], &melt)), records(&expected));
                    // Incomplete callers must recompute, never omit an unbounded shape.
                    assert_eq!(records(&group_info_with_bounds(&refs[..count], &[], &melt)), records(&expected));
                }
            }
        }
    }

    #[test]
    fn a_sphere_around_spheres_holds_them() {
        let items = [Bound { c: [0.0, 0.0, 0.0], r: 5.0 }, Bound { c: [30.0, 4.0, -9.0], r: 2.0 }, Bound { c: [-8.0, 20.0, 3.0], r: 7.0 }];
        let e = enclose(&items);
        for b in &items {
            let d = ((b.c[0] - e.c[0]).powi(2) + (b.c[1] - e.c[1]).powi(2) + (b.c[2] - e.c[2]).powi(2)).sqrt();
            assert!(d + b.r <= e.r + 1e-3, "{b:?} not inside {e:?}");
        }
        assert_eq!(enclose(&[]), Bound::NONE);
        let bad = enclose(&[Bound { c: [f32::NAN, 0.0, 0.0], r: 1.0 }]);
        assert!(bad.r.is_finite() && bad.c.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn blocks_are_cut_from_the_list_in_order() {
        let shapes: Vec<Shape> = (0..19).map(|i| Shape::new(Kind::Sphere, [i as f32 * 10.0, 0.0, 0.0], Material::default())).collect();
        let refs: Vec<&Shape> = shapes.iter().collect();
        let info = group_info(&refs, &Melt::default());
        assert_eq!(info.blocks.len(), 3);
        assert_eq!((info.blocks[0].first, info.blocks[0].count), (0, BLOCK));
        assert_eq!((info.blocks[2].first, info.blocks[2].count), (2 * BLOCK, 3));
        assert!(info.blocks.iter().all(|b| b.skippable));
        for (i, s) in shapes.iter().enumerate() {
            let b = &info.blocks[i / BLOCK];
            assert!(b.bound.lower(s.pos) < 0.0, "shape {i} is inside its block's sphere");
            assert!(info.bound.lower(s.pos) < 0.0);
        }
    }

    #[test]
    fn an_overlap_shape_makes_its_block_unskippable() {
        let mut shapes: Vec<Shape> = (0..3).map(|i| Shape::new(Kind::Sphere, [i as f32 * 10.0, 0.0, 0.0], Material::default())).collect();
        shapes[2].op = Op::Intersect;
        let refs: Vec<&Shape> = shapes.iter().collect();
        assert!(!group_info(&refs, &Melt::default()).blocks[0].skippable);
    }

    /// The numbers behind the skips: beyond `far_enough` the soft minimum returns `a` exactly, and
    /// a union never lowers the distance by more than `pull`.
    #[test]
    fn a_far_value_leaves_a_blend_alone_and_the_pull_is_bounded() {
        for mode in BlendMode::ALL {
            for k in [0.0_f32, 0.5, 3.0, 12.0, 40.0] {
                for a in [-30.0_f32, -8.0, 0.0, 2.0, 30.0] {
                    let b = far_enough(a, k, mode);
                    let (d, keep) = crate::sdf::smin(a, b, k, mode);
                    assert!((d - a).abs() < 1e-4 && keep > 0.999, "{mode:?} k {k} a {a}: {d}");
                    // block_far holds for the carve and the add side, and for any sign
                    let far = block_far(a, k, mode);
                    assert!(far >= far_enough(a, k, mode) - 1e-4 && far >= far_enough(-a, k, mode) - 1e-4, "{mode:?} k {k} a {a}");
                    assert!(far >= paint_far(k));
                }
                // (the skips only ever happen where both values are positive)
                for (a, b) in [(0.0_f32, 0.0_f32), (3.0, 2.0), (5.0, 5.0), (1.0, 0.0), (k * 0.5, k * 0.5), (k, 0.2), (0.0, 30.0)] {
                    let (d, _) = crate::sdf::smin(a, b, k, mode);
                    assert!(d >= a.min(b) - pull(mode, k) - 1e-4, "{mode:?} k {k}: smin({a},{b}) = {d}");
                }
            }
        }
    }
}
