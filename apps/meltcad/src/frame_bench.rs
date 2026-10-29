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

/// Measures actual CPU frame-body responsiveness while a large native file loads.
/// This is a UI-input benchmark, NOT a GPU benchmark. The file is deliberately padded
/// with valid JSON whitespace so disk reading and parsing overlap mouse movement.
/// `cargo test --release -p meltcad document_io_frame_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn document_io_frame_bench() {
    let sc = crate::bench::library_scene(300);
    let path = std::env::temp_dir().join(format!("yura_document_frame_{}.yura", std::process::id()));
    let mut payload = sc.to_json().into_bytes();
    payload.resize(payload.len() + 32 * 1024 * 1024, b' ');
    std::fs::write(&path, &payload).expect("prepare large document");
    drop(payload);

    let mut rig = Rig::new(Scene::default());
    let centre = viewport_centre(&rig);
    rig.app.open_path(path.clone());
    rig.times.clear();
    let begin = Instant::now();
    for k in 0..100 {
        // True egui pointer events while the document worker runs.
        let p = centre + vec2((k as f32 * 2.0).sin() * 100.0, (k as f32 * 0.1).cos() * 60.0);
        rig.frame(vec![Event::PointerMoved(p)]);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let (avg, p95, max) = rig.stats();
    eprintln!(
        "native document load + 100 pointer frames: {:.2}s total, CPU avg {:.3}ms p95 {:.3}ms max {:.3}ms",
        begin.elapsed().as_secs_f64(),
        avg,
        p95,
        max
    );
    // No arbitrary FPS or latency assertion: report actual machine/build values,
    // separately from the existing 100/300-shape and GPU timestamp benchmarks.
    assert!(avg.is_finite() && p95.is_finite() && max.is_finite());
    let _ = std::fs::remove_file(path);
}

/// The scene list scrolls with the mouse wheel when several groups are picked (it did not).
#[test]
fn the_scene_list_scrolls_while_several_groups_are_picked() {
    let mut sc = Scene::default();
    sc.groups.clear();
    for i in 0..40 {
        let mut g = crate::model::Group::new(format!("G{i}"));
        g.shapes.push(crate::model::Shape::new(crate::model::Kind::Sphere, [0.0; 3], crate::model::Material::default()));
        sc.groups.push(g);
    }
    let scroll_moves = |multi: bool| -> f32 {
        let mut r = Rig::new(sc.clone());
        r.frame(vec![]);
        r.app.sel = Some(Pick::Group(0));
        if multi {
            r.app.multi = vec![Pick::Group(1), Pick::Group(2)];
            r.app.multi_for = r.app.sel;
        }
        for _ in 0..3 {
            r.frame(vec![]);
        }
        let Some((_, rect)) = r.app.row_rects.first().copied() else { return -1.0 };
        let y0 = rect.top();
        let at = rect.center();
        r.frame(vec![Event::PointerMoved(at)]);
        for _ in 0..6 {
            r.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, -80.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
        }
        for _ in 0..4 {
            r.frame(vec![]);
        }
        let y1 = r.app.row_rects.first().map_or(y0, |(_, rc)| rc.top());
        y0 - y1
    };
    let single = scroll_moves(false);
    let several = scroll_moves(true);
    assert!(single > 20.0, "baseline: the list scrolls ({single})");
    assert!(several > 20.0, "with several groups picked the list must scroll too ({several})");
}

/// The wheel zooms the view while several groups are picked.
#[test]
fn the_wheel_zooms_while_several_groups_are_picked() {
    let mut sc = Scene::default();
    sc.groups.clear();
    for i in 0..4 {
        let mut g = crate::model::Group::new(format!("G{i}"));
        g.shapes.push(crate::model::Shape::new(crate::model::Kind::Sphere, [i as f32 * 30.0, 0.0, 0.0], crate::model::Material::default()));
        sc.groups.push(g);
    }
    let mut r = Rig::new(sc);
    r.app.sel = Some(Pick::Group(0));
    r.app.multi = vec![Pick::Group(1), Pick::Group(2)];
    r.app.multi_for = r.app.sel;
    for _ in 0..3 {
        r.frame(vec![]);
    }
    let c = viewport_centre(&r);
    r.frame(vec![Event::PointerMoved(c)]);
    let d0 = r.app.cam.dist;
    for _ in 0..5 {
        r.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, 3.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
    }
    for _ in 0..5 {
        r.frame(vec![]);
    }
    assert!((r.app.cam.dist - d0).abs() > 1.0, "the wheel must zoom ({d0} -> {})", r.app.cam.dist);
}
