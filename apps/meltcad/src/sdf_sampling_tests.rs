//! Exact regression and optional timing against the material evaluator that ran
//! before the distance-only specialization. The old nesting loop is retained as
//! an independent reference: every group, including leaves, uses its heap stack.

use super::*;
use crate::model::Group;

impl Field {
    fn reference_sample(&self, p: V3) -> Sample {
        use crate::accel::{far_enough, paint_far};
        let p = warp(p, self.drip, self.ripple);
        // groups nest: a group melts into the group it sits in, and that group (with all it holds)
        // melts into the scene. `stack` holds the groups that are still open.
        struct Open {
            depth: usize,
            op: Op,
            k: f32,
            acc: Option<Sample>,
        }
        fn fold(target: &mut Option<Sample>, gs: Sample, op: Op, k: f32, mode: BlendMode) {
            *target = match target.take() {
                None => {
                    if op != Op::Add {
                        return;
                    }
                    Some(gs)
                }
                Some(a) => Some(combine(a, gs, op, k, mode)),
            };
        }
        fn close(stack: &mut Vec<Open>, root: &mut Option<Sample>, mode: BlendMode) {
            if let Some(done) = stack.pop()
                && let Some(gs) = done.acc
            {
                let parent = match stack.last_mut() {
                    Some(o) => &mut o.acc,
                    None => root,
                };
                fold(parent, gs, done.op, done.k, mode);
            }
        }
        let mut root: Option<Sample> = None;
        let mut stack: Vec<Open> = Vec::new();
        for fg in &self.groups {
            let Some(src) = self.sources.get(fg.src) else { continue };
            let k = fg.blend * self.amount;
            for j in 0..fg.insts.len().max(1) {
                let inst = fg.insts.get(j);
                while stack.last().is_some_and(|o| o.depth >= fg.depth) {
                    close(&mut stack, &mut root, self.mode);
                }
                let parent_acc = match stack.last() {
                    Some(o) => o.acc.as_ref(),
                    None => root.as_ref(),
                };
                if let Some(a) = parent_acc
                    && fg.leaf
                {
                    let lb = match inst {
                        Some(i) => i.bound(src.info.bound).lower(p),
                        None => src.info.bound.lower(p),
                    };
                    let far = match fg.op {
                        Op::Intersect => false,
                        Op::Add => lb >= far_enough(a.dist, k, self.mode),
                        Op::Subtract => lb >= far_enough(-a.dist, k, self.mode),
                        Op::Paint => lb >= paint_far(k),
                    };
                    if far {
                        continue;
                    }
                }
                let (q, scale) = match inst {
                    Some(i) => (i.back(p), i.s),
                    None => (p, 1.0),
                };
                let mut own = self.group::<true>(src, q);
                if let Some(gs) = own.as_mut() {
                    gs.dist *= scale;
                    if fg.linked && gs.nearest.is_some() {
                        gs.nearest = Some((fg.gi, LINKED));
                    }
                }
                stack.push(Open { depth: fg.depth, op: fg.op, k, acc: own });
            }
        }
        while !stack.is_empty() {
            close(&mut stack, &mut root, self.mode);
        }
        let acc = root;
        let mut out = acc.unwrap_or(Sample { dist: 1e6, color: [0.8; 3], roughness: 0.5, metallic: 0.0, nearest: None });
        if self.drip > 0.0 {
            out.dist *= drip_safety(self.drip, self.ripple);
        }
        out
    }
}

fn reference_ray(field: &Field, origin: V3, dir: V3, max: f32) -> Option<(f32, Sample)> {
    let mut t = 0.0;
    for _ in 0..256 {
        let sample = field.reference_sample(add(origin, mul(dir, t)));
        if sample.dist < 0.02 * (1.0 + t * 0.002) {
            return Some((t, sample));
        }
        t += sample.dist.max(0.01);
        if t > max {
            return None;
        }
    }
    None
}

fn same_sample(actual: Sample, expected: Sample) {
    assert_eq!(actual.dist.to_bits(), expected.dist.to_bits(), "distance: {actual:?} / {expected:?}");
    assert_eq!(actual.color.map(f32::to_bits), expected.color.map(f32::to_bits), "surface colour");
    assert_eq!(actual.roughness.to_bits(), expected.roughness.to_bits(), "roughness");
    assert_eq!(actual.metallic.to_bits(), expected.metallic.to_bits(), "metallic");
    assert_eq!(actual.nearest, expected.nearest, "picking identity");
}

fn points(scene: &Scene, count: usize) -> Vec<V3> {
    let (centre, radius) = crate::gpu::bounds(scene, None);
    let radius = radius.max(40.0);
    let mut state = 0x31415926u32;
    let mut rnd = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    (0..count).map(|_| add(centre, [rnd() * radius * 2.4, rnd() * radius * 2.4, rnd() * radius * 2.4])).collect()
}

fn nested_scene() -> Scene {
    let mut scene = super::tests::busy_scene(9, 10, 117);
    for (gi, group) in scene.groups.iter_mut().enumerate() {
        group.depth = [0, 1, 2, 3, 2, 1, 0, 1, 0][gi];
        group.op = [Op::Add, Op::Subtract, Op::Add, Op::Paint, Op::Intersect, Op::Add, Op::Add, Op::Paint, Op::Subtract][gi];
        for (si, shape) in group.shapes.iter_mut().enumerate() {
            shape.material.gradient = 0.8;
            shape.material.color2 = [0.9, 0.3, 0.7];
            shape.material.grad_local = si % 2 == 0;
            shape.material.grad_axis = (si % 3) as u8;
            shape.shell = if si % 5 == 0 { 1.5 } else { 0.0 };
            if si % 4 == 3 {
                shape.op = Op::Paint;
            }
        }
    }
    scene.groups[5].pattern = Some(crate::pattern::Pattern::Rect { counts: [2, 2, 1], spacing: [25.0, 35.0, 0.0] });
    let source = scene.ensure_group_uid(0);
    let mut linked = Group::new("linked");
    linked.instance_of = source;
    linked.xform.pos = [35.0, 20.0, 12.0];
    scene.groups.push(linked);
    scene
}

#[test]
fn distance_and_materials_match_the_original_order_for_nested_patterns_and_stains() {
    let scenes = [Scene::default(), crate::bench::pipe_scene(), crate::bench::gradient_scene(), nested_scene()];
    for mut scene in scenes {
        let samples = points(&scene, 160);
        for mode in BlendMode::ALL {
            for drip in [0.0, 9.0] {
                scene.melt.mode = mode;
                scene.melt.drip = drip;
                scene.melt.ripple = 1.7;
                let field = Field::new(&scene);
                for &p in &samples {
                    let reference = field.reference_sample(p);
                    assert_eq!(field.distance(p).to_bits(), reference.dist.to_bits(), "geometry at {p:?}, {mode:?}, drip {drip}");
                    same_sample(field.sample(p), reference);
                }
                // Also sample the centre and near the surface of every source shape:
                // uniform world samples alone would miss small nested cutters/stains.
                for source in &field.sources {
                    for shape in &source.shapes {
                        for offset in [[0.0; 3], [shape.size[0], 0.0, 0.0], [0.0, 0.0, shape.size[1] * 0.5]] {
                            let p = add(shape.pos, offset);
                            let reference = field.reference_sample(p);
                            assert_eq!(field.distance(p).to_bits(), reference.dist.to_bits());
                            same_sample(field.sample(p), reference);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn library_geometry_and_ray_hits_keep_their_exact_materials_and_picking_identity() {
    for category in crate::library::all() {
        for item in &category.items {
            let scene = item.scene();
            let field = Field::new(&scene);
            let (centre, radius) = crate::gpu::bounds(&scene, None);
            for p in points(&scene, 8) {
                let reference = field.reference_sample(p);
                assert_eq!(field.distance(p).to_bits(), reference.dist.to_bits(), "{} at {p:?}", item.name);
                same_sample(field.sample(p), reference);
                let dir = norm(sub(centre, p));
                let reach = radius * 4.0 + 200.0;
                match (field.ray(p, dir, reach), reference_ray(&field, p, dir, reach)) {
                    (Some((t, sample)), Some((old_t, reference))) => {
                        assert_eq!(t.to_bits(), old_t.to_bits(), "ray hit for {}", item.name);
                        same_sample(sample, reference);
                    }
                    (None, None) => {}
                    (a, b) => panic!("ray changed for {}: {a:?} / {b:?}", item.name),
                }
            }
        }
    }
}

#[test]
fn empty_and_unusually_deep_groups_preserve_their_folding_order() {
    let mut scene = Scene::default();
    scene.groups.clear();
    same_sample(Field::new(&scene).sample([0.0; 3]), Field::new(&scene).reference_sample([0.0; 3]));
    // Unsanitized in-memory scenes can exceed the document depth cap. The optimization
    // must neither panic nor silently drop a group in that case.
    for depth in 0..=u8::MAX {
        let mut group = Group::new("nested");
        group.depth = depth;
        if depth % 7 == 0 {
            let mut shape = Shape::new(Kind::Sphere, [0.0; 3], Material::default());
            shape.size[0] = 20.0 + f32::from(depth) * 0.1;
            group.shapes.push(shape);
        }
        scene.groups.push(group);
    }
    let field = Field::new(&scene);
    for p in [[0.0; 3], [30.0, 0.0, 0.0], [100.0, -50.0, 20.0]] {
        let reference = field.reference_sample(p);
        same_sample(field.sample(p), reference);
        assert_eq!(field.distance(p).to_bits(), reference.dist.to_bits());
    }
}

#[test]
fn a_missing_link_between_a_leaf_and_a_deeper_group_keeps_the_resolved_order() {
    let mut scene = Scene::default();
    let mut cutter = scene.groups[0].clone();
    cutter.op = Op::Subtract;
    cutter.shapes[0].size[0] = 10.0;
    scene.groups.push(cutter);
    let mut missing = Group::new("missing link");
    missing.instance_of = Some(u32::MAX);
    scene.groups.push(missing);
    let mut child = scene.groups[0].clone();
    child.depth = 1;
    child.shapes[0].size[0] = 14.0;
    scene.groups.push(child);
    let field = Field::new(&scene);
    assert_eq!(field.groups.len(), 3, "the missing link is omitted during resolution");
    for p in [[0.0, 0.0, 15.0], [12.0, 0.0, 15.0], [28.0, 0.0, 15.0]] {
        let reference = field.reference_sample(p);
        same_sample(field.sample(p), reference);
        assert_eq!(field.distance(p).to_bits(), reference.dist.to_bits());
    }
}

#[test]
fn non_finite_in_memory_inputs_do_not_panic_or_change_distance_classification() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0] {
        for op in [Op::Add, Op::Subtract, Op::Intersect, Op::Paint] {
            let mut scene = Scene::default();
            let mut shape = scene.groups[0].shapes[0].clone();
            shape.op = op;
            shape.size[0] = value;
            shape.material.gradient = 1.0;
            shape.material.color2[0] = value;
            scene.groups[0].shapes.push(shape);
            let field = Field::new(&scene);
            for p in [[0.0; 3], [30.0, 0.0, 20.0], [value, 0.0, 0.0]] {
                let expected = field.reference_sample(p).dist;
                let actual = field.distance(p);
                if expected.is_nan() {
                    assert!(actual.is_nan(), "{op:?}, value {value}, at {p:?}");
                } else {
                    assert_eq!(actual.to_bits(), expected.to_bits(), "{op:?}, value {value}, at {p:?}");
                }
                let old_hit = reference_ray(&field, p, [0.0, 0.0, 1.0], 100.0);
                let new_hit = field.ray(p, [0.0, 0.0, 1.0], 100.0);
                assert_eq!(new_hit.is_some(), old_hit.is_some());
            }
        }
    }
}

/// CPU wall time only: `cargo test -p meltcad --release cpu_sampling_speed -- --ignored --nocapture`.
/// The reference evaluates materials at every sample and uses the old nesting stack;
/// the optimized path must have exactly the same distance checksum before timing.
#[test]
#[ignore]
fn cpu_sampling_speed() {
    use std::hint::black_box;
    use std::time::Instant;
    eprintln!("CPU field samples, 20000 points; medians of 7 interleaved runs (preparation excluded)");
    eprintln!("scene,shapes,reference_ms,distance_ms,ratio,sample_ms");
    for (name, scene) in [
        ("default", Scene::default()),
        ("gradient", crate::bench::gradient_scene()),
        ("nested", nested_scene()),
        ("shapes100", crate::bench::library_scene(100)),
        ("shapes300", crate::bench::library_scene(300)),
    ] {
        let field = Field::new(&scene);
        let samples = points(&scene, 20000);
        let measure = |which: usize| {
            let start = Instant::now();
            let sum = samples
                .iter()
                .map(|&p| match which {
                    0 => black_box(field.reference_sample(black_box(p))).dist,
                    1 => field.distance(black_box(p)),
                    _ => black_box(field.sample(black_box(p))).dist,
                })
                .fold(0.0_f64, |total, d| total + f64::from(d));
            (start.elapsed().as_secs_f64() * 1000.0, black_box(sum))
        };
        let mut times: [Vec<f64>; 3] = Default::default();
        let expected = measure(0).1;
        for run in 0..8 {
            for step in 0..3 {
                let which = (run + step) % 3;
                let (time, sum) = measure(which);
                assert_eq!(sum.to_bits(), expected.to_bits(), "checksum for {name}, path {which}");
                if run > 0 {
                    times[which].push(time);
                }
            }
        }
        for row in &mut times {
            row.sort_by(f64::total_cmp);
        }
        let [reference, distance, sample] = times.map(|row| row[3]);
        eprintln!("{name},{},{reference:.3},{distance:.3},{:.2},{sample:.3}", scene.shape_count(), reference / distance);
    }
}

/// Include the final material lookup in ray timings: doing geometry first must
/// still help when a ray hits quickly. This is CPU picking/ray tracing, not GPU FPS.
#[test]
#[ignore]
fn cpu_ray_speed() {
    use std::hint::black_box;
    eprintln!("CPU ray tracing, 64x40 view rays; medians of 7 interleaved runs (preparation excluded)");
    eprintln!("scene,shapes,reference_ms,optimized_ms,ratio");
    for (name, scene) in [
        ("default", Scene::default()),
        ("pipe", crate::bench::pipe_scene()),
        ("gradient", crate::bench::gradient_scene()),
        ("shapes100", crate::bench::library_scene(100)),
        ("shapes300", crate::bench::library_scene(300)),
    ] {
        let field = Field::new(&scene);
        let cam = crate::bench::camera_for(&scene);
        let rays: Vec<_> = (0..40)
            .flat_map(|y| (0..64).map(move |x| cam.ray([(x as f32 + 0.5) / 64.0 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / 40.0 * 2.0], 1.6)))
            .collect();
        let measure = |optimized| {
            let start = std::time::Instant::now();
            let mut sum = 0.0_f64;
            for &(origin, dir) in &rays {
                let hit = black_box(if optimized {
                    field.ray(black_box(origin), black_box(dir), 5000.0)
                } else {
                    reference_ray(&field, black_box(origin), black_box(dir), 5000.0)
                });
                sum += hit.map_or(0.0, |(t, _)| f64::from(t));
            }
            (start.elapsed().as_secs_f64() * 1000.0, black_box(sum))
        };
        let mut times: [Vec<f64>; 2] = Default::default();
        let expected = measure(false).1;
        for run in 0..8 {
            for step in 0..2 {
                let which = (run + step) % 2;
                let (elapsed, sum) = measure(which == 1);
                assert_eq!(sum.to_bits(), expected.to_bits(), "ray checksum for {name}");
                if run > 0 {
                    times[which].push(elapsed);
                }
            }
        }
        for row in &mut times {
            row.sort_by(f64::total_cmp);
        }
        let [reference, optimized] = times.map(|row| row[3]);
        eprintln!("{name},{},{reference:.3},{optimized:.3},{:.2}", scene.shape_count(), reference / optimized);
    }
}
