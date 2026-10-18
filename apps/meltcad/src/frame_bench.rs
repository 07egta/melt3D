//! How long the processor needs for a frame of the real window while the person drags things
//! around, with real pointer events through `App::frame_body` (top bar, panels and viewport), in a
//! window the size of the report that started this work (2528 x 1440 pixels).
//!
//! The ignored benchmark prints milliseconds per frame:
//! `cargo test -p meltcad --release frame_cpu_bench -- --ignored --nocapture`
//! The ordinary tests pin down the savings (no repack on a camera move, no field built while
//! dragging, no scene copies when nothing changed).

use std::time::Instant;

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};

use crate::app::App;
use crate::model::{Pick, Scene};

fn exact_cache_scene(count: usize, kind: crate::model::Kind) -> Scene {
    use crate::model::{Group, Material, Shape};
    let mut scene = Scene::default();
    scene.groups.clear();
    for first in (0..count).step_by(10) {
        let mut group = Group::new(format!("cache {}", first / 10));
        for i in first..(first + 10).min(count) {
            let mut shape = Shape::new(kind, [(i % 20) as f32 * 25.0, (i / 20) as f32 * 25.0, 10.0], Material::default());
            if kind == crate::model::Kind::Extrude {
                shape.outline = (0..48)
                    .map(|j| {
                        let a = j as f32 * std::f32::consts::TAU / 48.0;
                        [a.cos() * 8.0, a.sin() * 8.0]
                    })
                    .collect();
                shape.round = 0.8;
            } else if kind == crate::model::Kind::Curve {
                shape.path = (0..8).map(|j| [j as f32 * 2.5, (j as f32 * 0.6).sin() * 3.0, 0.0, 1.2]).collect();
            }
            group.shapes.push(shape);
        }
        scene.groups.push(group);
    }
    scene
}

#[test]
fn actual_app_caches_share_unchanged_geometry_and_keep_noop_updates_hot() {
    let mut app = App::new(true, None);
    app.scene = exact_cache_scene(100, crate::model::Kind::Extrude);
    let rev = app.track_scene();
    let first_field = app.field();
    let first_pack = app.perf.pack.get(&app.scene, rev);
    let field_builds = app.field_builds.get();
    for _ in 0..100 {
        let rev = app.track_scene();
        assert!(std::rc::Rc::ptr_eq(&first_field, &app.field()));
        assert!(std::sync::Arc::ptr_eq(&first_pack, &app.perf.pack.get(&app.scene, rev)));
    }
    assert_eq!(app.field_builds.get(), field_builds);
    assert_eq!(app.perf.pack.packs, 1);
    app.scene.groups[0].shapes[0].pos[0] += 1.0;
    let rev = app.track_scene();
    let changed_field = app.field();
    let changed_pack = app.perf.pack.get(&app.scene, rev);
    assert_eq!(changed_field.prepared_built, 1);
    assert_eq!(changed_field.prepared_reused, 99);
    assert_eq!(changed_pack.shapes_repacked, 1);
    assert_eq!(changed_pack.shapes_reused, 99);
    let cold = crate::gpu::ScenePack::new(&app.scene, crate::gpu::VolumeSet::of(&app.scene));
    assert_eq!(changed_pack.list.len(), cold.list.len());
    assert!(changed_pack.list.iter().zip(cold.list.iter()).all(|(a, b)| a.to_bits() == b.to_bits()));
}

#[test]
fn signed_zero_edits_match_cold_when_another_shape_advances_revision() {
    let mut app = App::new(true, None);
    app.scene = exact_cache_scene(20, crate::model::Kind::Extrude);
    app.scene.groups[0].shapes[0].material.color[0] = 0.0;
    let revision = app.track_scene();
    let before_field = app.field();
    let before_pack = app.perf.pack.get(&app.scene, revision);
    let changed = &mut app.scene.groups[0].shapes[0];
    changed.pos[0] = -0.0;
    changed.rot[0] = -0.0;
    changed.outline[0][1] = -0.0;
    changed.material.color[0] = -0.0;
    // The original scene-revision equivalence treats signed zeros as equal. A concurrent
    // ordinary edit makes the revision advance, and the new cache must still match cold.
    app.scene.groups[1].shapes[0].pos[0] += 0.5;
    let next_revision = app.track_scene();
    assert_ne!(next_revision, revision);
    let after_field = app.field();
    let after_pack = app.perf.pack.get(&app.scene, next_revision);
    let cold_field = crate::sdf::Field::new(&app.scene);
    let cold_pack = crate::gpu::ScenePack::new(&app.scene, crate::gpu::VolumeSet::of(&app.scene));
    assert_eq!(after_field.prepared_built, 2);
    assert_eq!(after_pack.shapes_repacked, 2);
    assert_eq!(after_field.sources[0].shapes[0].pos[0].to_bits(), (-0.0_f32).to_bits());
    assert_eq!(after_field.sources[0].shapes[0].outline[0][1].to_bits(), (-0.0_f32).to_bits());
    assert!(!std::sync::Arc::ptr_eq(&before_field.sources[0], &after_field.sources[0]));
    assert!(!std::sync::Arc::ptr_eq(&before_pack, &after_pack));
    for x in -10..10 {
        let p = [x as f32 * 0.7, 0.0, 10.0];
        assert_eq!(after_field.distance(p).to_bits(), cold_field.distance(p).to_bits());
    }
    assert_eq!(after_pack.list.len(), cold_pack.list.len());
    assert!(after_pack.list.iter().zip(cold_pack.list.iter()).all(|(a, b)| a.to_bits() == b.to_bits()));
}

/// Includes revision detection and the actual App/PackCache entrypoints, including retained
/// input snapshots. CPU preparation/packing is measured here, never GPU frame time.
#[test]
#[ignore]
fn exact_cache_update_bench() {
    use crate::gpu::{ScenePack, VolumeSet};
    use crate::model::Kind;
    use crate::sdf::Field;
    use std::time::Duration;
    const EDITS: usize = 30;
    for (label, kind) in [("primitives", Kind::Sphere), ("profiles48", Kind::Extrude), ("curves8", Kind::Curve)] {
        for count in [10, 100, 300] {
            let scene = exact_cache_scene(count, kind);
            // Retained input payload only: two independent CPU/GPU cache copies, excluding
            // allocator rounding and map/vector capacities. Mesh distance volumes stay shared.
            let input_bytes: usize = scene
                .groups
                .iter()
                .flat_map(|group| &group.shapes)
                .map(|shape| {
                    std::mem::size_of::<crate::model::Shape>()
                        + 2 * std::mem::size_of::<usize>()
                        + std::mem::size_of::<std::sync::Arc<crate::model::Shape>>()
                        + shape.name.len()
                        + shape.source.len()
                        + (shape.outline.len() + shape.outline2.len()) * std::mem::size_of::<[f32; 2]>()
                        + shape.path.len() * std::mem::size_of::<[f32; 4]>()
                })
                .sum();
            eprintln!("CACHE_INPUT_BYTES {label} shapes={count} two_caches={}", input_bytes * 2);
            let mut app = App::new(true, None);
            app.scene = scene.clone();
            let mut baseline = App::new(true, None);
            baseline.scene = scene;
            let rev = app.track_scene();
            let _ = app.field();
            let _ = app.perf.pack.get(&app.scene, rev);
            let _ = baseline.track_scene();
            let first_builds = app.field_builds.get();
            let first_packs = app.perf.pack.packs;
            let start = Instant::now();
            for _ in 0..1000 {
                let rev = app.track_scene();
                std::hint::black_box(app.field());
                std::hint::black_box(app.perf.pack.get(&app.scene, rev));
            }
            let noop_us = start.elapsed().as_secs_f64() * 1_000.0;
            assert_eq!(app.field_builds.get(), first_builds);
            assert_eq!(app.perf.pack.packs, first_packs);
            let mut cold_time = Duration::ZERO;
            let mut reuse_time = Duration::ZERO;
            for edit in 0..EDITS {
                let i = edit % count;
                app.scene.groups[i / 10].shapes[i % 10].pos[0] += 0.4;
                baseline.scene.groups[i / 10].shapes[i % 10].pos[0] += 0.4;
                // Alternate order to avoid making either path consistently the first/warm run.
                let mut cold = None;
                let mut incremental = None;
                for reuse in if edit % 2 == 0 { [false, true] } else { [true, false] } {
                    let start = Instant::now();
                    if reuse {
                        let rev = app.track_scene();
                        let field = app.field();
                        let pack = app.perf.pack.get(&app.scene, rev);
                        reuse_time += start.elapsed();
                        assert_eq!(field.prepared_built, 1);
                        incremental = Some((field, pack));
                    } else {
                        let _ = baseline.track_scene();
                        let field = Field::new(&baseline.scene);
                        let pack = ScenePack::new(&baseline.scene, VolumeSet::of(&baseline.scene));
                        cold_time += start.elapsed();
                        cold = Some((field, pack));
                    }
                }
                let (field, pack) = incremental.unwrap();
                let (cold_field, cold_pack) = cold.unwrap();
                assert_eq!(field.distance([12.0, 0.0, 10.0]).to_bits(), cold_field.distance([12.0, 0.0, 10.0]).to_bits());
                assert_eq!(pack.list.len(), cold_pack.list.len());
                assert!(pack.list.iter().zip(cold_pack.list.iter()).all(|(a, b)| a.to_bits() == b.to_bits()));
            }
            eprintln!(
                "cache {label:<11} {count:>3} shapes: actual no-op {noop_us:.3} us/update; cold {:.3} ms/edit; incremental {:.3} ms/edit; {:.2}x; {EDITS} edits (CPU only)",
                cold_time.as_secs_f64() * 1000.0 / EDITS as f64,
                reuse_time.as_secs_f64() * 1000.0 / EDITS as f64,
                cold_time.as_secs_f64() / reuse_time.as_secs_f64().max(1e-12)
            );
        }
    }
}

struct Rig {
    ctx: egui::Context,
    app: App,
    time: f64,
    /// Processor time of every frame (ms).
    times: Vec<f32>,
}

impl Rig {
    fn new(scene: Scene) -> Rig {
        let ctx = egui::Context::default();
        crate::style::apply(&ctx);
        ctx.set_pixels_per_point(2.0);
        let mut app = App::new(true, None);
        // the graphics path counts as ready: the frame packs the scene and hands it to a paint callback (which a test window never runs)
        app.gpu.easy = false;
        let generation = app.gpu.status.begin(Instant::now());
        app.gpu.status.finish_ok(generation, 0.0);
        app.new_scene(scene);
        let mut rig = Rig { ctx, app, time: 0.0, times: Vec::new() };
        for _ in 0..3 {
            rig.frame(vec![]);
        }
        rig.times.clear();
        rig
    }

    /// One full frame of the window; returns its processor time (ms).
    fn frame(&mut self, events: Vec<Event>) -> f32 {
        self.frame_with(events, Modifiers::NONE)
    }

    fn frame_with(&mut self, events: Vec<Event>, _modifiers: Modifiers) -> f32 {
        self.time += 0.0083;
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1264.0, 720.0))), events, time: Some(self.time), ..Default::default() };
        let t = Instant::now();
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.frame_body(ui));
        let ms = t.elapsed().as_secs_f32() * 1000.0;
        out.textures_delta.clear();
        self.times.push(ms);
        ms
    }

    fn press(&mut self, p: Pos2, button: PointerButton, down: bool, m: Modifiers) {
        self.frame_with(vec![Event::PointerButton { pos: p, button, pressed: down, modifiers: m }], m);
    }

    /// Press with `button` at `from`, move in `steps` frames to `to`, release.
    fn drag(&mut self, button: PointerButton, from: Pos2, to: Pos2, steps: usize, m: Modifiers) {
        self.frame_with(vec![Event::PointerMoved(from)], m);
        self.press(from, button, true, m);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            self.frame_with(vec![Event::PointerMoved(from + (to - from) * t)], m);
        }
        self.press(to, button, false, m);
        self.frame(vec![]);
    }

    fn stats(&self) -> (f32, f32, f32) {
        let mut v = self.times.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let avg = v.iter().sum::<f32>() / v.len() as f32;
        (avg, v[(v.len() * 95 / 100).min(v.len() - 1)], v[v.len() - 1])
    }
}

/// A scene of about `n` library shapes with a 12-node pipe in it, the pipe selected.
fn scene_with_pipe(n: usize) -> (Scene, Pick) {
    let mut sc = crate::bench::library_scene(n);
    let mut pipe = crate::bench::pipe_scene();
    let g = pipe.groups.remove(0);
    sc.groups.push(g);
    let gi = sc.groups.len() - 1;
    (sc, Pick::Shape(gi, 0))
}

fn pipe_node_screen(rig: &Rig, pick: Pick, k: usize) -> Option<Pos2> {
    let Pick::Shape(g, s) = pick else { return None };
    let shape = rig.app.scene.shape(g, s)?;
    let lay = crate::curve_tool::layout(&rig.app.cam, rig.app.view_rect, shape);
    lay.nodes.get(k).copied().flatten().map(|n| n.c)
}

fn viewport_centre(rig: &Rig) -> Pos2 {
    rig.app.view_rect.center()
}

#[test]
fn a_camera_move_packs_nothing_and_builds_no_field() {
    let (sc, pick) = scene_with_pipe(30);
    let mut r = Rig::new(sc);
    r.app.sel = Some(pick);
    r.frame(vec![]);
    let c = viewport_centre(&r);
    // (the first look of the mouse over the picture picks once, which builds the field)
    r.frame(vec![Event::PointerMoved(c)]);
    let packs = r.app.perf.pack.packs;
    let builds = r.app.field_builds.get();
    r.drag(PointerButton::Middle, c, c + vec2(120.0, 40.0), 20, Modifiers::NONE);
    r.frame(vec![]);
    assert_eq!(r.app.perf.pack.packs, packs, "orbiting must not pack the scene again");
    assert_eq!(r.app.field_builds.get(), builds, "orbiting builds no field for picking");
}

#[test]
fn dragging_a_node_does_not_build_the_field_each_frame() {
    let (sc, pick) = scene_with_pipe(30);
    let mut r = Rig::new(sc);
    r.app.sel = Some(pick);
    r.frame(vec![]);
    let Some(node) = pipe_node_screen(&r, pick, 5) else { panic!("node 5 on screen") };
    let before = r.app.scene.clone();
    let builds = r.app.field_builds.get();
    r.drag(PointerButton::Primary, node, node + vec2(40.0, -30.0), 30, Modifiers::NONE);
    assert_ne!(r.app.scene, before, "the node moved");
    let built = r.app.field_builds.get() - builds;
    assert!(built <= 3, "{built} fields built during a 30-frame drag (picking at the end may build one)");
}

#[test]
fn an_idle_window_copies_and_compares_nothing_new() {
    let (sc, pick) = scene_with_pipe(30);
    let mut r = Rig::new(sc);
    r.app.sel = Some(pick);
    for _ in 0..3 {
        r.frame(vec![]);
    }
    let packs = r.app.perf.pack.packs;
    let builds = r.app.field_builds.get();
    for _ in 0..20 {
        r.frame(vec![]);
    }
    assert_eq!(r.app.perf.pack.packs, packs);
    assert_eq!(r.app.field_builds.get(), builds);
}

#[test]
fn the_undo_step_is_made_once_when_the_mouse_is_up() {
    let (sc, pick) = scene_with_pipe(12);
    let mut r = Rig::new(sc);
    r.app.sel = Some(pick);
    r.frame(vec![]);
    let Some(node) = pipe_node_screen(&r, pick, 5) else { panic!("node on screen") };
    let before = r.app.scene.clone();
    r.drag(PointerButton::Primary, node, node + vec2(30.0, 0.0), 12, Modifiers::NONE);
    r.frame(vec![]);
    r.frame(vec![]);
    assert_ne!(r.app.scene, before);
    r.app.undo();
    assert_eq!(r.app.scene, before, "one drag is one undo step");
}

#[test]
fn the_hud_is_drawn_inside_the_viewport_when_switched_on() {
    let (sc, pick) = scene_with_pipe(5);
    let mut r = Rig::new(sc);
    r.app.sel = Some(pick);
    r.frame(vec![]);
    assert!(r.ctx.memory(|m| m.area_rect(egui::Id::new("frame_hud"))).is_none(), "off by default");
    r.app.perf.hud = true;
    for _ in 0..3 {
        r.frame(vec![]);
    }
    let Some(area) = r.ctx.memory(|m| m.area_rect(egui::Id::new("frame_hud"))) else { panic!("the HUD was not drawn") };
    let view = r.app.view_rect;
    assert!(area.width() > 100.0 && area.height() > 40.0, "{area:?}");
    assert!(view.contains_rect(area), "{area:?} not inside {view:?}");
    // the F3 key switches it
    r.app.perf.hud = false;
    r.frame(vec![egui::Event::Key { key: egui::Key::F3, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(r.app.perf.hud, "F3 switches the HUD on");
}

/// `cargo test -p meltcad --release frame_cpu_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn frame_cpu_bench() {
    for (label, n) in [("pipe only", 0usize), ("pipe + 20 shapes", 20), ("pipe + 100 shapes", 100), ("pipe + 300 shapes", 300)] {
        let (sc, pick) = scene_with_pipe(n);
        let shapes = sc.shape_count();
        let report = |what: &str, r: &Rig| {
            let (avg, p95, max) = r.stats();
            eprintln!(
                "{label:<18} {shapes:>4} shapes  {what:<14} cpu ms/frame: avg {avg:6.2}  p95 {p95:6.2}  max {max:6.2}  ({} frames, {} packs, {} fields)",
                r.times.len(),
                r.app.perf.pack.packs,
                r.app.field_builds.get()
            );
        };
        // idle
        let mut r = Rig::new(sc.clone());
        r.app.sel = Some(pick);
        r.frame(vec![]);
        r.times.clear();
        for _ in 0..60 {
            r.frame(vec![]);
        }
        report("idle", &r);
        // the camera orbits (middle button)
        r.times.clear();
        let c = viewport_centre(&r);
        r.drag(PointerButton::Middle, c, c + vec2(300.0, 80.0), 90, Modifiers::NONE);
        report("orbit", &r);
        // hover over the picture (the mouse moves with nothing pressed)
        r.times.clear();
        for k in 0..90 {
            r.frame_with(vec![Event::PointerMoved(c + vec2(k as f32 * 3.0 - 150.0, (k as f32 * 0.2).sin() * 60.0))], Modifiers::NONE);
        }
        report("hover", &r);
        // a node of the pipe is dragged
        r.times.clear();
        r.app.sel = Some(pick);
        r.frame(vec![]);
        if let Some(node) = pipe_node_screen(&r, pick, 5) {
            r.times.clear();
            r.drag(PointerButton::Primary, node, node + vec2(80.0, -40.0), 90, Modifiers::NONE);
            report("node drag", &r);
        }
        // the move handle of the whole shape
        r.times.clear();
        r.app.sel = Some(Pick::Shape(0, 0));
        r.frame(vec![]);
        if let Some(c) = crate::view::centre_of(&r.app.scene, r.app.sel)
            && let Some(p) = r.app.cam.project(r.app.view_rect, c)
        {
            r.times.clear();
            r.drag(PointerButton::Primary, p, p + vec2(60.0, 20.0), 90, Modifiers::NONE);
            report("gizmo drag", &r);
        }
    }
}
