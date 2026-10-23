//! The speed test (`meltcad --bench [out.txt]`, or "Speed test…" in the がしつ menu).
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
    let mut g = Group::new("Pipe");
    let nodes: Vec<[f32; 4]> = (0..12)
        .map(|i| {
            let t = i as f32 / 11.0;
            [-70.0 + 140.0 * t, 30.0 * (t * 7.0).sin(), 28.0 + 22.0 * (t * 5.0).cos(), 4.0]
        })
        .collect();
    let mut s = Shape::new(Kind::Curve, [0.0, 0.0, 0.0], Material::plain([0.62, 0.38, 0.92], 0.4, 0.0));
    s.name = "Pipe".to_string();
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
    let mut g = Group::new("Soft");
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
    // Matched pairs keep geometry, instances and camera framing unchanged; only glow differs.
    let unlit = |n: usize| {
        let mut scene = lib(n);
        for group in &mut scene.groups {
            for shape in &mut group.shapes {
                shape.material.glow = 0.0;
            }
        }
        scene
    };
    vec![
        BenchScene { name: "pipe", scene: pipe_scene() },
        BenchScene { name: "library20", scene: lib(20) },
        BenchScene { name: "shapes100", scene: lib(100) },
        BenchScene { name: "shapes100_unlit", scene: unlit(100) },
        BenchScene { name: "shapes300", scene: lib(300) },
        BenchScene { name: "shapes300_unlit", scene: unlit(300) },
        BenchScene { name: "gradient", scene: gradient_scene() },
    ]
}

#[cfg(test)]
mod performance_scene_tests {
    use super::*;

    #[test]
    fn matched_100_and_300_shape_scenes_have_identical_geometry() {
        let all = scenes();
        for n in [100, 300] {
            let lit_name = format!("shapes{n}");
            let unlit_name = format!("shapes{n}_unlit");
            let lit = all.iter().find(|s| s.name == lit_name).expect("lit scene");
            let unlit = all.iter().find(|s| s.name == unlit_name).expect("unlit scene");
            assert_eq!(lit.scene.shape_count(), unlit.scene.shape_count());
            assert_eq!(lit.scene.groups.len(), unlit.scene.groups.len());
            // the camera frame differs on purpose: a lamp's glow widens the bounds by its halo
            assert!(!crate::perf::scene_has_glow(&unlit.scene));
            for (a, b) in lit.scene.groups.iter().zip(&unlit.scene.groups) {
                assert_eq!(a.shapes.len(), b.shapes.len());
                for (a, b) in a.shapes.iter().zip(&b.shapes) {
                    let mut copy = a.clone();
                    copy.material.glow = 0.0;
                    assert_eq!(&copy, b);
                }
            }
        }
    }
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
        if r.timestamps {
            "GPU timestamp queries (ms = time the card spent on the scene pass)"
        } else {
            "wall clock around submit + wait (this device has no timestamp queries)"
        }
    ));
    out.push_str(&format!("shader    : compiled in {:.2} s\n", r.compile_secs));
    out.push_str(&format!("frames    : {} per row after {} warm-up\n", r.frames, WARMUP));
    if !r.pack_ms.is_empty() {
        let list: Vec<String> = r.pack_ms.iter().map(|(n, ms)| format!("{n} {}", ms_text(*ms))).collect();
        out.push_str(&format!("cpu pack  : {} ms per frame that changes the scene\n", list.join(", ")));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<10} {:>6}  {:<10} {:<5} {:>8} {:>8} {:>8} {:>7} {:>8}\n",
        "scene", "shapes", "size", "mode", "avg ms", "min", "max", "fps", "wall ms"
    ));
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
            out.push_str(&format!(
                "  {:<10} {:.2} x  ({:.0} fps at that size)\n",
                x.scene,
                expected_scale(x.avg_ms),
                1000.0 / (x.avg_ms * expected_scale(x.avg_ms).powi(2)).max(0.01)
            ));
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
pub fn run(
    sizes: &[[u32; 2]],
    frames: usize,
    scenes: &[BenchScene],
    progress: &mut dyn FnMut(&str),
    stop: &dyn Fn() -> bool,
    backends: Option<egui_wgpu::wgpu::Backends>,
) -> Result<Report, String> {
    run_with(sizes, frames, scenes, progress, stop, None, backends)
}

/// `run`, with another source for the scene shader (experiments).
pub fn run_with(
    sizes: &[[u32; 2]],
    frames: usize,
    scenes: &[BenchScene],
    progress: &mut dyn FnMut(&str),
    stop: &dyn Fn() -> bool,
    source: Option<&str>,
    backends: Option<egui_wgpu::wgpu::Backends>,
) -> Result<Report, String> {
    progress("Preparing graphics…");
    let mut gpu = crate::gpu::offscreen::Offscreen::open_with(source, backends)?;
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
                let (source, used) =
                    if gpu_ms.len() == wall_ms.len() && !gpu_ms.is_empty() { (Source::Gpu, &gpu_ms) } else { (Source::Wall, &wall_ms) };
                let (avg, min, max) = stats(used);
                let (wall_avg, _, _) = stats(&wall_ms);
                report.rows.push(Row {
                    scene: bs.name.to_string(),
                    shapes: bs.scene.shape_count(),
                    size,
                    mode,
                    frames,
                    avg_ms: avg,
                    min_ms: min,
                    max_ms: max,
                    source,
                    wall_avg_ms: wall_avg,
                });
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

/// The backend the window would start on (so that the test measures what the person's window uses):
/// `MELTCAD_BACKEND` if set, else the one that worked last, else the usual set.
pub fn backends_like_the_window() -> Option<egui_wgpu::wgpu::Backends> {
    let (choice, _) = crate::backend::choice_now();
    match crate::backend::plan(choice, cfg!(target_os = "windows"), &crate::backend::load(), &[]) {
        Some(crate::backend::Plan::Only(b)) => Some(b.to_wgpu()),
        _ => None,
    }
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

/// Handle `--bench`: None for other commands, Some(true) only when
/// measurement and report persistence both succeeded.
pub fn cli(args: &[std::ffi::OsString]) -> Option<bool> {
    let a = parse_args(args)?;
    let sizes: &[[u32; 2]] = if a.quick { &SIZES[..1] } else { &SIZES };
    let frames = if a.quick { a.frames.min(3) } else { a.frames };
    let mut progress = |s: &str| eprintln!("[bench] {s}");
    let result = run(sizes, frames, &scenes(), &mut progress, &|| false, backends_like_the_window());
    let (title, text, path, success) = match result {
        Ok(r) => {
            let text = format_table(&r);
            let path = a.out.clone().unwrap_or_else(default_output);
            let written = std::fs::write(&path, &text);
            let success = written.is_ok();
            let note = match written {
                Ok(()) => format!("\n\n(saved: {})", path.display()),
                Err(e) => format!("\n\n(could not save {}: {e})", path.display()),
            };
            println!("{text}");
            ("MELTCAD speed test", format!("{text}{note}"), Some(path), success)
        }
        Err(e) => ("MELTCAD speed test", format!("Could not measure:\n{e}"), None, false),
    };
    eprintln!("[bench] done {}", path.map(|p| p.display().to_string()).unwrap_or_default());
    // (a release build on Windows has no console: the answer is shown in a dialog too)
    if std::env::var_os("MELTCAD_BENCH_QUIET").is_none() {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title(title)
            .set_description(text)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    Some(success)
}

// ---------------------------------------------------------------------------------------------
// The window ("Speed test…")

use std::sync::{Arc, Mutex};

/// The test running in the window on its own thread.
#[derive(Default)]
pub struct Job {
    shared: Arc<Mutex<JobState>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub open: bool,
    /// The backend the window uses (the test draws with the same).
    pub last_backends: Option<egui_wgpu::wgpu::Backends>,
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
    pub fn start(&mut self, ctx: &egui::Context, backends: Option<egui_wgpu::wgpu::Backends>) {
        self.open = true;
        self.last_backends = backends;
        if self.lock().running {
            return;
        }
        *self.lock() = JobState { running: true, progress: "Starting…".to_string(), ..Default::default() };
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
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&SIZES, FRAMES, &scenes(), &mut progress, &|| stop.load(std::sync::atomic::Ordering::Relaxed), backends)
            }));
            let (result, saved) = match outcome {
                Ok(Ok(r)) => {
                    let text = format_table(&r);
                    let path = default_output();
                    let saved = std::fs::write(&path, &text).map(|()| path.display().to_string()).map_err(|e| format!("{}: {e}", path.display()));
                    (
                        Ok(text),
                        Some(match saved {
                            Ok(p) => format!("Saved: {p}"),
                            Err(e) => format!("Could not save: {e}"),
                        }),
                    )
                }
                Ok(Err(e)) => (Err(e), None),
                Err(_) => (Err("An error happened during the test (panic)".to_string()), None),
            };
            tell(&|st| {
                st.running = false;
                st.result = Some(result.clone());
                st.saved = saved.clone();
            });
        });
        if spawned.is_err() {
            *self.lock() = JobState { result: Some(Err("Could not start a thread".to_string())), ..Default::default() };
        }
    }

    /// The window with the progress or the table.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let st = self.lock().clone();
        let mut open = self.open;
        let mut restart = false;
        egui::Window::new("Speed test").open(&mut open).default_width(640.0).default_height(480.0).show(ctx, |ui| {
            if st.running {
                ui.label("Measuring. Please don't touch this window or the view until it finishes.");
                ui.label(egui::RichText::new(&st.progress).monospace());
                ui.add(egui::Spinner::new());
                if ui.button("Stop").clicked() {
                    self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                return;
            }
            match &st.result {
                Some(Ok(text)) => {
                    ui.horizontal(|ui| {
                        if ui.button("Copy").on_hover_text("Copy the table to the clipboard").clicked() {
                            ui.ctx().copy_text(text.clone());
                        }
                        if ui.button("Try again").clicked() {
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
                    ui.label(egui::RichText::new("Could not measure").strong());
                    ui.label(e);
                    if ui.button("Try again").clicked() {
                        restart = true;
                    }
                }
                None => {
                    ui.label("Not measured yet.");
                    if ui.button("Start").clicked() {
                        restart = true;
                    }
                }
            }
        });
        self.open = open;
        if restart {
            self.start(ctx, self.last_backends);
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
        assert_eq!(names, ["pipe", "library20", "shapes100", "shapes100_unlit", "shapes300", "shapes300_unlit", "gradient"]);
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
        let row = |scene: &str, size: [u32; 2], mode, avg: f32| Row {
            scene: scene.to_string(),
            shapes: 1,
            size,
            mode,
            frames: 12,
            avg_ms: avg,
            min_ms: avg * 0.9,
            max_ms: avg * 1.3,
            source: Source::Gpu,
            wall_avg_ms: avg + 0.4,
        };
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
        for want in [
            "speed test",
            "Intel(R) Arc(TM) 140V",
            "Vulkan",
            "GPU timestamp queries",
            "1.25 s",
            "pipe 0.05",
            "shapes300 1.23",
            "2560x1440",
            "avg ms",
            "fast",
            "full",
            "41.0",
            "160",
            "3.20",
        ] {
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
        assert_eq!(cli(&a(&[])), None);
        assert_eq!(parse_args(&a(&["scene.meltcad"])), None);
        assert_eq!(parse_args(&a(&["--bench"])), Some(Args { out: None, frames: FRAMES, quick: false }));
        assert_eq!(
            parse_args(&a(&["--bench", "r.txt", "--frames", "5", "--quick"])),
            Some(Args { out: Some("r.txt".into()), frames: 5, quick: true })
        );
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
        match run_with(&sizes, frames, &scenes, &mut progress, &|| false, shader_experiment().as_deref(), None) {
            Ok(r) => println!("{}", format_table(&r)),
            Err(e) => eprintln!("no adapter: {e}"),
        }
    }

    /// `MELTCAD_SHADER_SED="old=>new||old2=>new2"`: the shader with text replaced, to see what a part of it costs
    /// (every `old` must occur in the source).
    fn shader_experiment() -> Option<String> {
        shader_experiment_on(&crate::gpu::shader_source(), "MELTCAD_SHADER_SED")
    }

    fn shader_experiment_on(source: &str, var: &str) -> Option<String> {
        let spec = std::env::var(var).ok()?;
        let mut src = source.to_string();
        for pair in spec.split("||") {
            let (old, new) = pair.split_once("=>")?;
            assert!(src.contains(old), "the shader has no {old:?}");
            src = src.replace(old, new);
        }
        Some(src)
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

    /// Writes the benchmark scenes as files the window can open:
    /// `MELTCAD_BENCH_OUT=/tmp/scenes cargo test -p meltcad --release dump_bench_scenes -- --ignored`
    #[test]
    #[ignore]
    fn dump_bench_scenes() {
        let Some(dir) = std::env::var_os("MELTCAD_BENCH_OUT").map(std::path::PathBuf::from) else {
            eprintln!("set MELTCAD_BENCH_OUT");
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        for s in scenes() {
            let _ = std::fs::write(dir.join(format!("{}.meltcad", s.name)), s.scene.to_json());
        }
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
        let mut gpu = match crate::gpu::offscreen::Offscreen::open_with(Some(&counting_source()), None) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("no adapter: {e}");
                return;
            }
        };
        println!(
            "{:<10} {:>6} {:<9} {:<5} {:>12} {:>9} {:>12} {:>9} {:>9}",
            "scene", "shapes", "size", "mode", "reads/px", "p95", "evals/px", "p95", "max"
        );
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

    /// Scenes for the comparison of the march with and without the per-ray candidates: patterns of
    /// every kind, a linked copy, a carved group, an overlap shape, a group that melts into a
    /// carved one with a big blend, and the library scenes.
    fn parity_scenes() -> Vec<(&'static str, Scene)> {
        use crate::model::Op;
        use crate::pattern::{Pattern, Xform};
        let mut out: Vec<(&'static str, Scene)> = Vec::new();
        let mut sc = Scene::default();
        sc.groups.clear();
        let mut boxes = Group::new("boxes");
        let mut b = Shape::new(Kind::Box, [30.0, 60.0, 15.0], Material::default());
        b.size = [25.0, 25.0, 25.0];
        boxes.shapes.push(b);
        boxes.pattern = Some(Pattern::Mirror { axes: [true, true, false], centre: false });
        sc.groups.push(boxes);
        let mut ring = Group::new("ring");
        let mut c = Shape::new(Kind::Cylinder, [50.0, -20.0, 0.0], Material::default());
        c.size = [8.0, 30.0, 1.0];
        let mut c2 = Shape::new(Kind::Sphere, [50.0, -20.0, 20.0], Material::default());
        c2.size[0] = 10.0;
        ring.shapes.push(c);
        ring.shapes.push(c2);
        ring.pattern = Some(Pattern::Circular { count: 5, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: true });
        sc.groups.push(ring);
        let mut balls = Group::new("balls");
        balls.shapes.push(Shape::new(Kind::Sphere, [-20.0, -20.0, 12.0], Material::default()));
        balls.pattern = Some(Pattern::Rect { counts: [4, 3, 2], spacing: [25.0, 25.0, 30.0] });
        let uid = sc.ensure_group_uid(0).unwrap_or(0);
        sc.groups.push(balls);
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [0.0, 0.0, 70.0], rot: [0.0, 0.0, 45.0], scale: 1.5 };
        sc.groups.push(copy);
        // a carving group and a group that keeps only the overlap
        let mut cut = Group::new("cut");
        cut.op = Op::Subtract;
        cut.blend = 4.0;
        let mut k = Shape::new(Kind::Sphere, [30.0, 60.0, 15.0], Material::default());
        k.size[0] = 14.0;
        cut.shapes.push(k);
        sc.groups.push(cut);
        out.push(("patterns", sc.clone()));
        let mut lens = Group::new("lens");
        lens.op = Op::Intersect;
        let mut l = Shape::new(Kind::Sphere, [0.0, 0.0, 30.0], Material::default());
        l.size[0] = 160.0;
        lens.shapes.push(l);
        sc.groups.push(lens);
        out.push(("pat+lens", sc));
        // a carver and an intersect shape inside one group, with big blends
        let mut one = Scene::default();
        one.groups.clear();
        let mut g = Group::new("g");
        for i in 0..6 {
            let mut s = Shape::new(Kind::Sphere, [i as f32 * 14.0 - 35.0, (i % 2) as f32 * 8.0, 18.0], Material::default());
            s.size[0] = 12.0;
            s.blend = 8.0;
            g.shapes.push(s);
        }
        let mut carve = Shape::new(Kind::Box, [0.0, 0.0, 22.0], Material::default());
        carve.op = Op::Subtract;
        carve.size = [10.0, 60.0, 10.0];
        carve.blend = 3.0;
        g.shapes.push(carve);
        one.groups.push(g.clone());
        out.push(("blends", one.clone()));
        let mut keep = Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material::default());
        keep.size[0] = 48.0;
        keep.op = Op::Intersect;
        keep.blend = 2.0;
        g.shapes.push(keep);
        one.groups[0] = g;
        out.push(("blend+keep", one));
        for b in scenes() {
            if b.name != "gradient" {
                out.push((b.name, b.scene));
            }
        }
        out
    }

    /// The shader that writes where the ray hit (distance as a 24-bit number in RGB, alpha 255 for a hit)
    /// instead of the picture, with the candidates on or off.
    fn depth_source(candidates: bool) -> String {
        let mut src = crate::gpu::shader_source();
        let on = "        use_cand = true;\n        list_ok = !ovf;";
        assert!(src.contains(on));
        // MELTCAD_BENCH_VARIANT=bits: the groups' bits alone (no shape list) for the "with" side
        let bits = std::env::var("MELTCAD_BENCH_VARIANT").is_ok_and(|v| v == "bits");
        if !candidates {
            src = src.replace(on, "        use_cand = false;\n        list_ok = false;");
        } else if bits {
            src = src.replace(on, "        use_cand = true;\n        list_ok = false;");
        }
        if candidates && let Some(extra) = shader_experiment_on(&src, "MELTCAD_SHADER_SED") {
            src = extra;
        }
        if let Some(extra) = shader_experiment_on(&src, "MELTCAD_SHADER_SED_ALL") {
            src = extra;
        }
        let end = "    return vec4<f32>(encode(col), 1.0);\n}";
        assert!(src.contains(end));
        if let Ok(custom) = std::env::var("MELTCAD_BENCH_END") {
            return src.replace(end, &format!("    {custom}\n}}"));
        }
        src.replace(end, "    let tq = u32(clamp(t * 256.0, 0.0, 16777215.0));\n    return vec4<f32>(f32(tq & 255u) / 255.0, f32((tq >> 8u) & 255u) / 255.0, f32((tq >> 16u) & 255u) / 255.0, select(0.0, 1.0, hit));\n}")
    }

    /// The per-ray candidates leave out only what cannot matter: the distance at which every pixel's
    /// ray hits is the same with them as with the full scan (to the march's own tolerance), and the
    /// same pixels hit.
    /// `cargo test -p meltcad --release candidates_match_the_full_scan -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn candidates_match_the_full_scan() {
        let (mut with, mut without) = match (
            crate::gpu::offscreen::Offscreen::open_with(Some(&depth_source(true)), None),
            crate::gpu::offscreen::Offscreen::open_with(Some(&depth_source(false)), None),
        ) {
            (Ok(a), Ok(b)) => (a, b),
            _ => {
                eprintln!("no adapter");
                return;
            }
        };
        let gold_src = {
            let mut g = depth_source(false);
            for (a, b) in [("var relax = select(1.1, 1.5, fast);", "var relax = 0.9;"), ("var max_it = select(128, 48, fast);", "var max_it = 400;")]
            {
                g = g.replace(a, b);
            }
            g
        };
        let mut gold =
            if std::env::var("MELTCAD_BENCH_GOLD").is_ok() { crate::gpu::offscreen::Offscreen::open_with(Some(&gold_src), None).ok() } else { None };
        let size = [320u32, 200u32];
        for (name, scene) in parity_scenes() {
            let cam = camera_for(&scene);
            let pack = crate::gpu::ScenePack::new(&scene, crate::gpu::VolumeSet::of(&scene));
            if std::env::var("MELTCAD_BENCH_DUMP").is_ok_and(|v| v == name) {
                let mut n = 0;
                for (gi, g) in scene.groups.iter().enumerate() {
                    for (si, sh) in g.shapes.iter().enumerate() {
                        println!(
                            "shape {n:>3} (g{gi} s{si}) {:?} pos {:?} size {:?} scale {:?} rot {:?} op {:?} blend {} round {} radius {:.1}",
                            sh.kind,
                            sh.pos,
                            sh.size,
                            sh.scale,
                            sh.rot,
                            sh.op,
                            sh.blend,
                            sh.round,
                            crate::perf::shape_radius(sh)
                        );
                        n += 1;
                    }
                }
            }
            let fast = std::env::var("MELTCAD_BENCH_FAST").is_ok();
            // (what the march itself leaves open: a fraction of a pixel's footprint at the distance of the hit, more while moving)
            let footprint = 2.0 * crate::camera::TAN_HALF_FOV / size[1] as f32;
            let tol_at = |t: f32| (t * footprint * if fast { 3.0 } else { 1.5 }).max(if fast { 1.5 } else { 0.5 });
            let mut results = Vec::new();
            for gpu in [&mut with, &mut without] {
                gpu.load(&pack);
                let pic = gpu.picture(size);
                let mut globals = pack.globals(&scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
                crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, fast, pack.glow));
                gpu.frame(&pic, &globals, fast);
                results.push(gpu.read(&pic));
            }
            if let Some(gold) = gold.as_mut() {
                gold.load(&pack);
                let pic = gold.picture(size);
                let mut globals = pack.globals(&scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
                crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, fast, pack.glow));
                gold.frame(&pic, &globals, fast);
                let g = gold.read(&pic);
                let hit = |p: &[u8]| p[3] > 127;
                let t_of = |p: &[u8]| (u32::from(p[0]) + (u32::from(p[1]) << 8) + (u32::from(p[2]) << 16)) as f32 / 256.0;
                for (tag, img) in [("candidates", &results[0]), ("full scan", &results[1])] {
                    let (mut wrong_hit, mut wrong_depth, mut n) = (0, 0, 0);
                    for (p, q) in img.chunks(4).zip(g.chunks(4)) {
                        if hit(q) {
                            n += 1;
                        }
                        if hit(p) != hit(q) {
                            wrong_hit += 1;
                        } else if hit(p) && (t_of(p) - t_of(q)).abs() > tol_at(t_of(q)) {
                            wrong_depth += 1;
                        }
                    }
                    println!("    against the careful march: {tag}: {wrong_hit} pixels hit differently, {wrong_depth} at another depth (of {n})");
                }
            }
            let (a, b) = (&results[0], &results[1]);
            if let Some(dir) = std::env::var_os("MELTCAD_BENCH_OUT") {
                for (tag, px) in [("with", a), ("without", b)] {
                    let mut pgm = format!("P5\n{} {}\n255\n", size[0], size[1]).into_bytes();
                    pgm.extend(px.chunks(4).map(|p| if p[3] > 127 { 40 + (p[1] / 2) } else { 0 }));
                    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}_{tag}.pgm")), pgm);
                    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
                    ppm.extend(px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
                    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}_{tag}.ppm")), ppm);
                }
            }
            let decode = |p: &[u8]| (p[3] > 127, (u32::from(p[0]) + (u32::from(p[1]) << 8) + (u32::from(p[2]) << 16)) as f32 / 256.0);
            let (w, h) = (size[0] as usize, size[1] as usize);
            let px = |img: &Vec<u8>, x: usize, y: usize| decode(&img[(y * w + x) * 4..(y * w + x) * 4 + 4]);
            let (mut both, mut only_a, mut only_b, mut inner, mut far, mut worst) = (0, 0, 0, 0, 0, 0.0_f32);
            for y in 0..h {
                for x in 0..w {
                    let ((ha, ta), (hb, tb)) = (px(a, x, y), px(b, x, y));
                    match (ha, hb) {
                        (true, true) => both += 1,
                        (true, false) => only_a += 1,
                        (false, true) => only_b += 1,
                        _ => {}
                    }
                    // away from silhouettes (a ray that grazes a surface hits it anywhere along a long stretch)
                    if ha && hb && x > 0 && y > 0 && x + 1 < w && y + 1 < h {
                        let all = (y - 1..=y + 1).all(|yy| (x - 1..=x + 1).all(|xx| px(a, xx, yy).0 && px(b, xx, yy).0));
                        if all {
                            inner += 1;
                            worst = worst.max((ta - tb).abs());
                            if (ta - tb).abs() > tol_at(tb) {
                                far += 1;
                            }
                        }
                    }
                }
            }
            println!(
                "{name:<10} {both:>6} hit by both ({inner} away from the silhouette), {only_a} only with candidates, {only_b} only without; away from the silhouette the worst distance difference is {worst:.3} mm, {far} pixels differ by more than the march's own tolerance"
            );
            assert!(both > 200, "{name}: the scene is in the picture");
            assert!(only_a + only_b <= both / 12 + 2, "{name}: about the same pixels hit (the rest is silhouette)");
            assert!(far <= inner / 300 + 1, "{name}: the hits are at the same distance");
        }
    }

    /// The picture with the shadow rays reading only what is near them is the picture with them reading everything.
    /// `cargo test -p meltcad --release shadow_candidates_match_the_full_scan -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn shadow_candidates_match_the_full_scan() {
        let on = crate::gpu::shader_source();
        let marker = "    if (G.offs2.z < 0.5) {\n        find_candidates(ro, rd, t, tend, 1.0 + G.melt.z + 0.1 * tend);";
        assert!(on.contains(marker), "the shadow's candidates are where the test looks for them");
        let off = on.replace(marker, "    if (false) {\n        find_candidates(ro, rd, t, tend, 1.0 + G.melt.z + 0.1 * tend);");
        let (mut with, mut without) =
            match (crate::gpu::offscreen::Offscreen::open_with(Some(&on), None), crate::gpu::offscreen::Offscreen::open_with(Some(&off), None)) {
                (Ok(a), Ok(b)) => (a, b),
                _ => {
                    eprintln!("no adapter");
                    return;
                }
            };
        let size = [320u32, 200u32];
        for (name, scene) in parity_scenes() {
            let cam = camera_for(&scene);
            let pack = crate::gpu::ScenePack::new(&scene, crate::gpu::VolumeSet::of(&scene));
            let mut images = Vec::new();
            for gpu in [&mut with, &mut without] {
                gpu.load(&pack);
                let pic = gpu.picture(size);
                let mut globals = pack.globals(&scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
                crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, false, pack.glow));
                gpu.frame(&pic, &globals, false);
                images.push(gpu.read(&pic));
            }
            let differing =
                images[0].chunks(4).zip(images[1].chunks(4)).filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > 6)).count();
            let total = (size[0] * size[1]) as usize;
            println!("{name:<10} {differing} of {total} pixels differ by more than 6/255 with the shadows reading less");
            assert!(differing * 500 <= total, "{name}: the shadows are the same");
        }
    }

    /// Writes a picture of every benchmark scene (full quality) as PPM files into `$MELTCAD_BENCH_OUT`
    /// (with `MELTCAD_SHADER_DIR` for another version of the shader): to look at, and to compare two versions.
    /// `cargo test -p meltcad --release render_bench_scenes -- --ignored`
    #[test]
    #[ignore]
    fn render_bench_scenes() {
        let Some(dir) = std::env::var_os("MELTCAD_BENCH_OUT").map(std::path::PathBuf::from) else {
            eprintln!("set MELTCAD_BENCH_OUT");
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let Ok(mut gpu) = crate::gpu::offscreen::Offscreen::open_with(None, None) else {
            eprintln!("no adapter");
            return;
        };
        let size = [640u32, 400u32];
        let fast = std::env::var("MELTCAD_BENCH_FAST").is_ok();
        for bs in scenes() {
            let cam = camera_for(&bs.scene);
            let pack = crate::gpu::ScenePack::new(&bs.scene, crate::gpu::VolumeSet::of(&bs.scene));
            gpu.load(&pack);
            let pic = gpu.picture(size);
            let mut globals = pack.globals(&bs.scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
            crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, fast, pack.glow));
            gpu.frame(&pic, &globals, fast);
            let px = gpu.read(&pic);
            let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
            ppm.extend(px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
            let _ = std::fs::write(dir.join(format!("{}.ppm", bs.name)), ppm);
        }
    }
}
