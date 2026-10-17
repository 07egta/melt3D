//! The speed test (`meltcad --bench [out.txt]`, or "速さをはかる…" in the がしつ menu).
//!
//! It draws a fixed set of scenes (a pipe like the ones people trace, a 20-shape library model, a
//! 100-shape and a 300-shape scene, and a scene of rounded shapes with gradients and glass) at
//! three window sizes, full quality and in the cheap mode used while things move, in a texture it
//! owns, with the same pipelines as the window. The time of each frame comes from timestamp queries
//! when the graphics card has them, else from the wall clock around the submit (after waiting for
//! the card). The result is a plain-text table that a person can send to us.
//!
//! The scene builders and the table are plain code (tested without a graphics card); the drawing
//! needs an adapter (`bench_on_this_machine`, an ignored test, runs it on the software one).

use std::time::Instant;

use crate::camera::Camera;
use crate::model::{Group, Kind, Material, Scene, Shape};

/// Window sizes of the test (pixels).
pub const SIZES: [[u32; 2]; 3] = [[1280, 720], [1920, 1080], [2560, 1440]];
/// Frames drawn per cell of the table, and frames thrown away before them.
pub const FRAMES: usize = 12;
pub const WARMUP: usize = 2;

/// A scene of the test.
pub struct BenchScene {
    pub name: &'static str,
    pub scene: Scene,
}

fn tidy_env(scene: &mut Scene) {
    // (as a person sees a new document: floor, grid, two lights)
    scene.env = Default::default();
}

/// A pipe like the one in the report that started this: a hollow purple tube with a dozen nodes.
pub fn pipe_scene() -> Scene {
    let mut scene = Scene::default();
    scene.groups.clear();
    let mut g = Group::new("パイプ");
    let nodes: Vec<[f32; 4]> = (0..12)
        .map(|i| {
            let t = i as f32 / 11.0;
            [-70.0 + 140.0 * t, 30.0 * (t * 7.0).sin(), 28.0 + 22.0 * (t * 5.0).cos(), 4.0]
        })
        .collect();
    let mut s = Shape::new(Kind::Curve, [0.0, 0.0, 0.0], Material::plain([0.62, 0.38, 0.92], 0.4, 0.0));
    s.name = "パイプ".to_string();
    s.path = nodes;
    s.params = [0.0, 1.0, 0.0, 1.0];
    s.blend = 2.0;
    s.shell = 1.5;
    g.shapes.push(s);
    scene.groups.push(g);
    tidy_env(&mut scene);
    scene
}

/// Rounded shapes with gradients, glass, metal and a lamp: the materials that cost the most.
pub fn gradient_scene() -> Scene {
    let mut scene = Scene::default();
    scene.groups.clear();
    let mut g = Group::new("やわらか");
    let mut a = Shape::new(Kind::Box, [-24.0, 0.0, 16.0], Material::plain([0.95, 0.4, 0.5], 0.3, 0.0));
    a.size = [30.0, 30.0, 30.0];
    a.round = 9.0;
    a.material.color2 = [0.3, 0.5, 1.0];
    a.material.gradient = 1.0;
    a.material.grad_axis = 2;
    g.shapes.push(a);
    let mut b = Shape::new(Kind::Sphere, [8.0, 6.0, 20.0], Material::plain([0.7, 0.9, 1.0], 0.1, 0.0));
    b.size[0] = 18.0;
    b.material.glass = 0.8;
    b.material.coat = 0.5;
    g.shapes.push(b);
    let mut c = Shape::new(Kind::Torus, [30.0, -4.0, 14.0], Material::plain([0.9, 0.8, 0.3], 0.25, 0.9));
    c.material.iridescence = 0.6;
    g.shapes.push(c);
    let mut d = Shape::new(Kind::Cylinder, [-4.0, 30.0, 14.0], Material::plain([0.4, 0.9, 0.6], 0.5, 0.0));
    d.round = 4.0;
    d.material.translucency = 0.7;
    d.material.color2 = [1.0, 0.9, 0.3];
    d.material.gradient = 0.8;
    g.shapes.push(d);
    scene.groups.push(g);
    tidy_env(&mut scene);
    scene
}

/// About `shapes` shapes: library models (cycling through every category) on a grid.
pub fn library_scene(shapes: usize) -> Scene {
    let items: Vec<&crate::library::Item> = crate::library::all().iter().flat_map(|c| c.items.iter()).collect();
    let mut sc = Scene::default();
    sc.groups.clear();
    if items.is_empty() {
        return sc;
    }
    let mut i = 0usize;
    while sc.shape_count() < shapes && sc.groups.len() < crate::model::MAX_GROUPS {
        let Some(it) = items.get((i * 7) % items.len()) else { break };
        if !it.fits(sc.groups.len(), sc.shape_count()) {
            break;
        }
        let side = ((shapes as f32 / 12.0).sqrt().ceil() as usize).max(1);
        let at = [((i % side) as f32 - side as f32 / 2.0) * 90.0, ((i / side) as f32 - side as f32 / 2.0) * 90.0];
        let mut g: Group = it.placed(at);
        g.name = format!("{} {i}", it.name);
        sc.groups.push(g);
        i += 1;
    }
    sc
}

/// The scenes of the test, in the order of the table.
pub fn scenes() -> Vec<BenchScene> {
    let lib = |n: usize| {
        let mut s = library_scene(n);
        tidy_env(&mut s);
        s
    };
    vec![
        BenchScene { name: "pipe", scene: pipe_scene() },
        BenchScene { name: "library20", scene: lib(20) },
        BenchScene { name: "shapes100", scene: lib(100) },
        BenchScene { name: "shapes300", scene: lib(300) },
        BenchScene { name: "gradient", scene: gradient_scene() },
    ]
}

/// The camera that frames a scene.
pub fn camera_for(scene: &Scene) -> Camera {
    let mut cam = Camera::default();
    let (c, r) = crate::gpu::bounds(scene, None);
    cam.frame(c, r.max(10.0));
    cam
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Full,
    Fast,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Full => "full",
            Mode::Fast => "fast",
        }
    }
}

/// Where the milliseconds of a row come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Timestamp queries around the scene pass.
    Gpu,
    /// The wall clock from submit until the card was done.
    Wall,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub scene: String,
    pub shapes: usize,
    pub size: [u32; 2],
    pub mode: Mode,
    pub frames: usize,
    pub avg_ms: f32,
    pub min_ms: f32,
    pub max_ms: f32,
    pub source: Source,
    /// The wall-clock average too (it includes the submit and the wait).
    pub wall_avg_ms: f32,
}

impl Row {
    pub fn fps(&self) -> f32 {
        if self.avg_ms > 0.0 { 1000.0 / self.avg_ms } else { 0.0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub version: String,
    pub when: String,
    pub adapter: String,
    pub backend: String,
    pub software: bool,
    pub timestamps: bool,
    pub compile_secs: f64,
    pub frames: usize,
    pub rows: Vec<Row>,
    /// (scene, ms to pack the scene and its per-frame numbers on the processor)
    pub pack_ms: Vec<(String, f32)>,
    pub notes: Vec<String>,
}

/// "2026-10-17 12:34 UTC" from seconds since 1970 (no calendar crate: days to a civil date by the usual algorithm).
pub fn utc_text(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

fn now_text() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    utc_text(secs)
}

fn ms_text(v: f32) -> String {
    if !v.is_finite() {
        "-".to_string()
    } else if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

/// The scale the adaptive resolution would settle on for a fast frame of `ms` at full size.
pub fn expected_scale(ms_at_full: f32) -> f32 {
    if !ms_at_full.is_finite() || ms_at_full <= 0.0 {
        return 1.0;
    }
    (crate::adapt::GPU_TARGET_MS / ms_at_full).sqrt().clamp(crate::adapt::MIN_SCALE, 1.0)
}

/// The table, as text a person can paste into a message.
pub fn format_table(r: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("MELTCAD {} speed test　{}\n", r.version, r.when));
    out.push_str(&format!("adapter   : {}\n", r.adapter));
    out.push_str(&format!("backend   : {}{}\n", r.backend, if r.software { "  (software renderer, not a real graphics card)" } else { "" }));
    out.push_str(&format!(
        "timing    : {}\n",
        if r.timestamps { "GPU timestamp queries (ms = time the card spent on the scene pass)" } else { "wall clock around submit + wait (this device has no timestamp queries)" }
    ));
    out.push_str(&format!("shader    : compiled in {:.2} s\n", r.compile_secs));
    out.push_str(&format!("frames    : {} per row after {} warm-up\n", r.frames, WARMUP));
    if !r.pack_ms.is_empty() {
        let list: Vec<String> = r.pack_ms.iter().map(|(n, ms)| format!("{n} {}", ms_text(*ms))).collect();
        out.push_str(&format!("cpu pack  : {} ms per frame that changes the scene\n", list.join(", ")));
    }
    out.push('\n');
    out.push_str(&format!("{:<10} {:>6}  {:<10} {:<5} {:>8} {:>8} {:>8} {:>7} {:>8}\n", "scene", "shapes", "size", "mode", "avg ms", "min", "max", "fps", "wall ms"));
    let mut last_scene = String::new();
    for row in &r.rows {
        if !last_scene.is_empty() && last_scene != row.scene {
            out.push('\n');
        }
        last_scene = row.scene.clone();
        out.push_str(&format!(
            "{:<10} {:>6}  {:<10} {:<5} {:>8} {:>8} {:>8} {:>7.1} {:>8}\n",
            row.scene,
            row.shapes,
            format!("{}x{}", row.size[0], row.size[1]),
            row.mode.label(),
            ms_text(row.avg_ms),
            ms_text(row.min_ms),
            ms_text(row.max_ms),
            row.fps(),
            ms_text(row.wall_avg_ms)
        ));
    }
    // what this means for the window
    let big = SIZES.iter().max_by_key(|s| s[0]).copied().unwrap_or([2560, 1440]);
    let fast_rows: Vec<&Row> = r.rows.iter().filter(|x| x.mode == Mode::Fast && x.size == big).collect();
    if !fast_rows.is_empty() {
        out.push_str(&format!("\nwhile moving at {}x{} the adaptive resolution would settle at about:\n", big[0], big[1]));
        for x in fast_rows {
            out.push_str(&format!("  {:<10} {:.2} x  ({:.0} fps at that size)\n", x.scene, expected_scale(x.avg_ms), 1000.0 / (x.avg_ms * expected_scale(x.avg_ms).powi(2)).max(0.01)));
        }
    }
    for n in &r.notes {
        out.push_str(&format!("\nnote: {n}\n"));
    }
    out
}

/// Mean, least and greatest of a list of milliseconds.
fn stats(v: &[f32]) -> (f32, f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let sum: f32 = v.iter().sum();
    (sum / v.len() as f32, v.iter().cloned().fold(f32::MAX, f32::min), v.iter().cloned().fold(0.0_f32, f32::max))
}

/// Run the test. `sizes`: window sizes; `frames`: frames per row; `progress` is told what is being
/// drawn; `stop` (checked between rows) ends it early with what is done so far.
pub fn run(sizes: &[[u32; 2]], frames: usize, scenes: &[BenchScene], progress: &mut dyn FnMut(&str), stop: &dyn Fn() -> bool) -> Result<Report, String> {
    progress("グラフィックスを準備しています…");
    let mut gpu = crate::gpu::offscreen::Offscreen::open()?;
    let mut report = Report {
        version: env!("CARGO_PKG_VERSION").to_string(),
        when: now_text(),
        adapter: gpu.adapter.clone(),
        backend: gpu.backend.clone(),
        software: gpu.software,
        timestamps: gpu.timing,
        compile_secs: gpu.compile_secs,
        frames,
        ..Default::default()
    };
    for bs in scenes {
        let cam = camera_for(&bs.scene);
        // the processor's side of a frame that changes the scene
        let t = Instant::now();
        let mut pack = crate::gpu::ScenePack::new(&bs.scene, crate::gpu::VolumeSet::of(&bs.scene));
        for _ in 0..4 {
            pack = crate::gpu::ScenePack::new(&bs.scene, crate::gpu::VolumeSet::of(&bs.scene));
            let _ = pack.globals(&bs.scene, &cam, [1280.0, 720.0], None, None, false);
        }
        report.pack_ms.push((bs.name.to_string(), t.elapsed().as_secs_f32() * 1000.0 / 5.0));
        gpu.load(&pack);
        for &size in sizes {
            let pic = gpu.picture(size);
            let mut globals = pack.globals(&bs.scene, &cam, [pic.size[0] as f32, pic.size[1] as f32], None, None, false);
            for mode in [Mode::Full, Mode::Fast] {
                if stop() {
                    report.notes.push("stopped early".to_string());
                    return Ok(report);
                }
                progress(&format!("{} {}x{} {}", bs.name, size[0], size[1], mode.label()));
                crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, mode == Mode::Fast, pack.glow));
                let mut gpu_ms = Vec::new();
                let mut wall_ms = Vec::new();
                for k in 0..WARMUP + frames {
                    let (wall, g) = gpu.frame(&pic, &globals, mode == Mode::Fast);
                    if k >= WARMUP {
                        wall_ms.push(wall as f32 * 1000.0);
                        if let Some(g) = g {
                            gpu_ms.push(g);
                        }
                    }
                }
                let (source, used) = if gpu_ms.len() == wall_ms.len() && !gpu_ms.is_empty() { (Source::Gpu, &gpu_ms) } else { (Source::Wall, &wall_ms) };
                let (avg, min, max) = stats(used);
                let (wall_avg, _, _) = stats(&wall_ms);
                report.rows.push(Row { scene: bs.name.to_string(), shapes: bs.scene.shape_count(), size, mode, frames, avg_ms: avg, min_ms: min, max_ms: max, source, wall_avg_ms: wall_avg });
            }
        }
    }
    Ok(report)
}

/// Where the result file goes by default: next to the program, else the temporary folder.
pub fn default_output() -> std::path::PathBuf {
    let name = "meltcad-bench.txt";
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        // (a program folder that cannot be written to, such as Program Files, falls through)
        let probe = dir.join(".meltcad-write-test");
        if std::fs::write(&probe, b"x").is_ok() {
            let _ = std::fs::remove_file(&probe);
            return dir.join(name);
        }
    }
    std::env::temp_dir().join(name)
}

/// What the command line asked for.
#[derive(Debug, PartialEq)]
pub struct Args {
    pub out: Option<std::path::PathBuf>,
    pub frames: usize,
    pub quick: bool,
}

/// `meltcad --bench [out.txt] [--frames N] [--quick]`
pub fn parse_args(args: &[std::ffi::OsString]) -> Option<Args> {
    if args.get(1).and_then(|a| a.to_str()) != Some("--bench") {
        return None;
    }
    let mut out = None;
    let mut frames = FRAMES;
    let mut quick = false;
    let mut i = 2;
    while let Some(a) = args.get(i) {
        match a.to_str() {
            Some("--quick") => quick = true,
            Some("--frames") => {
                i += 1;
                if let Some(n) = args.get(i).and_then(|v| v.to_str()).and_then(|v| v.parse::<usize>().ok()) {
                    frames = n.clamp(1, 1000);
                }
            }
            Some(_) => out = Some(std::path::PathBuf::from(a)),
            None => {}
        }
        i += 1;
    }
    Some(Args { out, frames, quick })
}

/// Handle `--bench`: returns true when the arguments were a benchmark request (the program then ends).
pub fn cli(args: &[std::ffi::OsString]) -> bool {
    let Some(a) = parse_args(args) else { return false };
    let sizes: &[[u32; 2]] = if a.quick { &SIZES[..1] } else { &SIZES };
    let frames = if a.quick { a.frames.min(3) } else { a.frames };
    let mut progress = |s: &str| eprintln!("[bench] {s}");
    let result = run(sizes, frames, &scenes(), &mut progress, &|| false);
    let (title, text, path) = match result {
        Ok(r) => {
            let text = format_table(&r);
            let path = a.out.clone().unwrap_or_else(default_output);
            let note = match std::fs::write(&path, &text) {
                Ok(()) => format!("\n\n(saved: {})", path.display()),
                Err(e) => format!("\n\n(could not save {}: {e})", path.display()),
            };
            println!("{text}");
            ("MELTCAD 速さのテスト", format!("{text}{note}"), Some(path))
        }
        Err(e) => ("MELTCAD 速さのテスト", format!("測れませんでした:\n{e}"), None),
    };
    eprintln!("[bench] done {}", path.map(|p| p.display().to_string()).unwrap_or_default());
    // (a release build on Windows has no console: the answer is shown in a dialog too)
    if std::env::var_os("MELTCAD_BENCH_QUIET").is_none() {
        rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(title).set_description(text).set_buttons(rfd::MessageButtons::Ok).show();
    }
    true
}

// ---------------------------------------------------------------------------------------------
// The window ("速さをはかる…")

use std::sync::{Arc, Mutex};

/// The test running in the window on its own thread.
#[derive(Default)]
pub struct Job {
    shared: Arc<Mutex<JobState>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub open: bool,
}

#[derive(Default, Clone)]
struct JobState {
    running: bool,
    progress: String,
    result: Option<Result<String, String>>,
    saved: Option<String>,
}

impl Job {
    fn lock(&self) -> std::sync::MutexGuard<'_, JobState> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start the test (does nothing when it is already running).
    pub fn start(&mut self, ctx: &egui::Context) {
        self.open = true;
        if self.lock().running {
            return;
        }
        *self.lock() = JobState { running: true, progress: "はじめています…".to_string(), ..Default::default() };
        self.stop.store(false, std::sync::atomic::Ordering::Relaxed);
        let shared = self.shared.clone();
        let stop = self.stop.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("meltcad-bench".into()).spawn(move || {
            let tell = |f: &dyn Fn(&mut JobState)| {
                let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
                f(&mut g);
                drop(g);
                ctx.request_repaint();
            };
            let mut progress = |s: &str| {
                let s = s.to_string();
                tell(&|st| st.progress = s.clone());
            };
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&SIZES, FRAMES, &scenes(), &mut progress, &|| stop.load(std::sync::atomic::Ordering::Relaxed))));
            let (result, saved) = match outcome {
                Ok(Ok(r)) => {
                    let text = format_table(&r);
                    let path = default_output();
                    let saved = std::fs::write(&path, &text).map(|()| path.display().to_string()).map_err(|e| format!("{}: {e}", path.display()));
                    (Ok(text), Some(match saved {
                        Ok(p) => format!("保存しました: {p}"),
                        Err(e) => format!("保存できませんでした: {e}"),
                    }))
                }
                Ok(Err(e)) => (Err(e), None),
                Err(_) => (Err("テスト中にエラーがおきました (panic)".to_string()), None),
            };
            tell(&|st| {
                st.running = false;
                st.result = Some(result.clone());
                st.saved = saved.clone();
            });
        });
        if spawned.is_err() {
            *self.lock() = JobState { result: Some(Err("スレッドを作れませんでした".to_string())), ..Default::default() };
        }
    }

    pub fn running(&self) -> bool {
        self.lock().running
    }

    /// The window with the progress or the table.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let st = self.lock().clone();
        let mut open = self.open;
        let mut restart = false;
        egui::Window::new("速さをはかる").open(&mut open).default_width(640.0).default_height(480.0).show(ctx, |ui| {
            if st.running {
                ui.label("測っています。終わるまで、この窓や絵にさわらないでください。");
                ui.label(egui::RichText::new(&st.progress).monospace());
                ui.add(egui::Spinner::new());
                if ui.button("やめる").clicked() {
                    self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                return;
            }
            match &st.result {
                Some(Ok(text)) => {
                    ui.horizontal(|ui| {
                        if ui.button("コピー").on_hover_text("表をクリップボードに").clicked() {
                            ui.ctx().copy_text(text.clone());
                        }
                        if ui.button("もう一度").clicked() {
                            restart = true;
                        }
                        if let Some(s) = &st.saved {
                            ui.label(egui::RichText::new(s).size(12.0));
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(12.0)).extend());
                    });
                }
                Some(Err(e)) => {
                    ui.label(egui::RichText::new("測れませんでした").strong());
                    ui.label(e);
                    if ui.button("もう一度").clicked() {
                        restart = true;
                    }
                }
                None => {
                    ui.label("まだ測っていません。");
                    if ui.button("はじめる").clicked() {
                        restart = true;
                    }
                }
            }
        });
        self.open = open;
        if restart {
            self.start(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scenes_have_the_sizes_the_test_promises() {
        let all = scenes();
        let names: Vec<&str> = all.iter().map(|s| s.name).collect();
        assert_eq!(names, ["pipe", "library20", "shapes100", "shapes300", "gradient"]);
        let count = |n: &str| all.iter().find(|s| s.name == n).map(|s| s.scene.shape_count()).unwrap_or(0);
        assert_eq!(count("pipe"), 1);
        assert!((18..=40).contains(&count("library20")), "{}", count("library20"));
        assert!((95..=140).contains(&count("shapes100")), "{}", count("shapes100"));
        assert!((295..=340).contains(&count("shapes300")), "{}", count("shapes300"));
        assert!(count("gradient") >= 4);
        for s in &all {
            let (_, r) = crate::gpu::bounds(&s.scene, None);
            assert!(r > 5.0 && r.is_finite(), "{} has a bounding sphere", s.name);
            let cam = camera_for(&s.scene);
            assert!(cam.dist.is_finite() && cam.dist > 4.0);
        }
    }

    #[test]
    fn the_pipe_is_a_hollow_curve_of_twelve_nodes() {
        let sc = pipe_scene();
        let Some(sh) = sc.shape(0, 0) else { panic!("the pipe") };
        assert_eq!(sh.kind, Kind::Curve);
        assert_eq!(sh.path.len(), 12);
        assert!(sh.shell > 0.0);
        let pts = crate::curve::build(&sh.path, &sh.params);
        assert!(pts.len() >= 80, "the smoothed pipe has {} points", pts.len());
    }

    #[test]
    fn the_scenes_pack_without_trouble() {
        for s in scenes() {
            let pack = crate::gpu::ScenePack::new(&s.scene, crate::gpu::VolumeSet::of(&s.scene));
            let g = pack.globals(&s.scene, &camera_for(&s.scene), [640.0, 360.0], None, None, false);
            assert!(g.iter().all(|v| v.is_finite()), "{}", s.name);
            assert!(pack.shape_n >= 1 && !pack.list.is_empty());
        }
    }

    #[test]
    fn dates_are_right() {
        assert_eq!(utc_text(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_text(951_782_400 + 3600 * 13 + 60 * 5), "2000-02-29 13:05 UTC");
        assert_eq!(utc_text(1_760_700_000), "2025-10-17 11:20 UTC");
    }

    fn sample_report() -> Report {
        let row = |scene: &str, size: [u32; 2], mode, avg: f32| Row { scene: scene.to_string(), shapes: 1, size, mode, frames: 12, avg_ms: avg, min_ms: avg * 0.9, max_ms: avg * 1.3, source: Source::Gpu, wall_avg_ms: avg + 0.4 };
        Report {
            version: "0.11.0".into(),
            when: "2026-10-17 12:00 UTC".into(),
            adapter: "Intel(R) Arc(TM) 140V (driver 32.0)".into(),
            backend: "Vulkan".into(),
            timestamps: true,
            compile_secs: 1.25,
            frames: 12,
            pack_ms: vec![("pipe".into(), 0.05), ("shapes300".into(), 1.234)],
            rows: vec![
                row("pipe", [1280, 720], Mode::Full, 41.0),
                row("pipe", [1280, 720], Mode::Fast, 9.0),
                row("pipe", [2560, 1440], Mode::Full, 160.0),
                row("pipe", [2560, 1440], Mode::Fast, 36.0),
                row("shapes100", [2560, 1440], Mode::Fast, 3.2),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_table_has_what_a_person_must_send_us() {
        let t = format_table(&sample_report());
        for want in ["speed test", "Intel(R) Arc(TM) 140V", "Vulkan", "GPU timestamp queries", "1.25 s", "pipe 0.05", "shapes300 1.23", "2560x1440", "avg ms", "fast", "full", "41.0", "160", "3.20"] {
            assert!(t.contains(want), "missing {want:?} in:\n{t}");
        }
        // the lines of the table line up
        let rows: Vec<&str> = t.lines().filter(|l| l.contains(" full ") || l.contains(" fast ")).collect();
        assert!(rows.len() >= 4);
        assert!(rows.windows(2).all(|w| w[0].len() == w[1].len()), "{rows:?}");
        // 36 ms at full size -> about 0.53x to hold 10 ms
        assert!(t.contains("0.53 x"), "{t}");
        assert!(t.contains("1.00 x"), "a 3.2 ms scene stays at full size:\n{t}");
    }

    #[test]
    fn a_wall_clock_table_says_so() {
        let mut r = sample_report();
        r.timestamps = false;
        r.software = true;
        let t = format_table(&r);
        assert!(t.contains("wall clock") && t.contains("software renderer"));
    }

    #[test]
    fn the_scale_a_scene_would_get() {
        assert_eq!(expected_scale(5.0), 1.0);
        assert!((expected_scale(40.0) - 0.5).abs() < 1e-6);
        assert_eq!(expected_scale(10_000.0), crate::adapt::MIN_SCALE);
        assert_eq!(expected_scale(f32::NAN), 1.0);
    }

    #[test]
    fn the_arguments() {
        let a = |v: &[&str]| -> Vec<std::ffi::OsString> { std::iter::once("meltcad").chain(v.iter().copied()).map(Into::into).collect() };
        assert_eq!(parse_args(&a(&[])), None);
        assert_eq!(parse_args(&a(&["scene.meltcad"])), None);
        assert_eq!(parse_args(&a(&["--bench"])), Some(Args { out: None, frames: FRAMES, quick: false }));
        assert_eq!(parse_args(&a(&["--bench", "r.txt", "--frames", "5", "--quick"])), Some(Args { out: Some("r.txt".into()), frames: 5, quick: true }));
        assert_eq!(parse_args(&a(&["--bench", "--frames", "99999"])).map(|x| x.frames), Some(1000));
        assert_eq!(parse_args(&a(&["--bench", "--frames", "x"])).map(|x| x.frames), Some(FRAMES));
    }

    #[test]
    fn the_output_goes_somewhere_writable() {
        let p = default_output();
        assert!(p.file_name().is_some_and(|n| n == "meltcad-bench.txt"));
    }

    #[test]
    fn statistics_of_nothing_are_zero() {
        assert_eq!(stats(&[]), (0.0, 0.0, 0.0));
        let (a, lo, hi) = stats(&[2.0, 4.0, 9.0]);
        assert_eq!((a, lo, hi), (5.0, 2.0, 9.0));
    }

    /// The whole test on this machine's adapter (the software one in a build box), at one size.
    /// `cargo test -p meltcad --release bench_on_this_machine -- --ignored --nocapture`
    /// (`MELTCAD_BENCH_FRAMES=n`, `MELTCAD_BENCH_SIZES=640x360,1280x720`, `MELTCAD_BENCH_SCENES=pipe,gradient`)
    #[test]
    #[ignore]
    fn bench_on_this_machine() {
        let frames = std::env::var("MELTCAD_BENCH_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let sizes: Vec<[u32; 2]> = std::env::var("MELTCAD_BENCH_SIZES")
            .ok()
            .map(|v| v.split(',').filter_map(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))).collect())
            .unwrap_or_else(|| vec![[640, 360]]);
        let only: Option<Vec<String>> = std::env::var("MELTCAD_BENCH_SCENES").ok().map(|v| v.split(',').map(str::to_string).collect());
        let scenes: Vec<BenchScene> = scenes().into_iter().filter(|s| only.as_ref().is_none_or(|o| o.iter().any(|n| n == s.name))).collect();
        let mut progress = |s: &str| eprintln!("[bench] {s}");
        match run(&sizes, frames, &scenes, &mut progress, &|| false) {
            Ok(r) => println!("{}", format_table(&r)),
            Err(e) => eprintln!("no adapter: {e}"),
        }
    }

    /// The shader with counters: every read of the scene list and every distance evaluation of a
    /// pixel is counted, and the counts become the pixel's colour (R, G = reads low and high byte,
    /// B = evaluations). Reads of the scene list are what an integrated graphics card runs out of
    /// first, and the numbers do not depend on the machine.
    fn counting_source() -> String {
        let src = crate::gpu::shader_source();
        let fetch = "fn D(i: i32) -> vec4<f32> { return textureLoad(DT, vec2<i32>(i & 1023, i >> 10), 0); }";
        assert!(src.contains(fetch), "the scene-list reader is the one the counters wrap");
        src.replace(fetch, "var<private> n_fetch: u32;\nvar<private> n_eval: u32;\nfn D(i: i32) -> vec4<f32> { n_fetch = n_fetch + 1u; return textureLoad(DT, vec2<i32>(i & 1023, i >> 10), 0); }")
            .replace("fn map_d(p0: vec3<f32>) -> f32 {\n", "fn map_d(p0: vec3<f32>) -> f32 {\n    n_eval = n_eval + 1u;\n")
            .replace("fn map(p0: vec3<f32>) -> Hit {\n", "fn map(p0: vec3<f32>) -> Hit {\n    n_eval = n_eval + 1u;\n")
            .replace(
                "    return vec4<f32>(encode(col), 1.0);\n}",
                "    return vec4<f32>(f32(n_fetch & 255u) / 255.0, f32((n_fetch >> 8u) & 255u) / 255.0, f32(min(n_eval, 255u)) / 255.0, 1.0);\n}",
            )
    }

    /// Reads of the scene list and distance evaluations per pixel, for every scene, full and fast.
    /// `cargo test -p meltcad --release count_work -- --ignored --nocapture`
    /// (`MELTCAD_BENCH_SIZES=w x h` as for the timing test; default 320x180)
    #[test]
    #[ignore]
    fn count_work() {
        let sizes: Vec<[u32; 2]> = std::env::var("MELTCAD_BENCH_SIZES")
            .ok()
            .map(|v| v.split(',').filter_map(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))).collect())
            .unwrap_or_else(|| vec![[320, 180]]);
        let only: Option<Vec<String>> = std::env::var("MELTCAD_BENCH_SCENES").ok().map(|v| v.split(',').map(str::to_string).collect());
        let mut gpu = match crate::gpu::offscreen::Offscreen::open_with(Some(&counting_source())) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("no adapter: {e}");
                return;
            }
        };
        println!("{:<10} {:>6} {:<9} {:<5} {:>12} {:>9} {:>12} {:>9} {:>9}", "scene", "shapes", "size", "mode", "reads/px", "p95", "evals/px", "p95", "max");
        for bs in scenes().into_iter().filter(|s| only.as_ref().is_none_or(|o| o.iter().any(|n| n == s.name))) {
            let cam = camera_for(&bs.scene);
            let pack = crate::gpu::ScenePack::new(&bs.scene, crate::gpu::VolumeSet::of(&bs.scene));
            gpu.load(&pack);
            for &size in &sizes {
                let pic = gpu.picture(size);
                let mut globals = pack.globals(&bs.scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
                for mode in [Mode::Full, Mode::Fast] {
                    crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, mode == Mode::Fast, pack.glow));
                    gpu.frame(&pic, &globals, mode == Mode::Fast);
                    let px = gpu.read(&pic);
                    let mut reads: Vec<u32> = px.chunks(4).map(|p| u32::from(p[0]) + (u32::from(p[1]) << 8)).collect();
                    let mut evals: Vec<u32> = px.chunks(4).map(|p| u32::from(p[2])).collect();
                    let mean = |v: &[u32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len().max(1) as f64;
                    let (mr, me) = (mean(&reads), mean(&evals));
                    reads.sort_unstable();
                    evals.sort_unstable();
                    let p95 = |v: &[u32]| v.get(v.len() * 95 / 100).copied().unwrap_or(0);
                    println!(
                        "{:<10} {:>6} {:<9} {:<5} {:>12.1} {:>9} {:>12.1} {:>9} {:>9}",
                        bs.name,
                        bs.scene.shape_count(),
                        format!("{}x{}", size[0], size[1]),
                        mode.label(),
                        mr,
                        p95(&reads),
                        me,
                        p95(&evals),
                        evals.last().copied().unwrap_or(0)
                    );
                }
            }
        }
    }
}
