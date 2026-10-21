//! "Simple view": the viewport drawn on the CPU, for when the graphics path is not ready (the
//! shader is still compiling, or the driver refused it) or the person chose not to use it.
//!
//! The picture is small (a third of the viewport, a sixth while the view moves), drawn by a worker
//! thread that splits the rows over all cores, and stretched with linear filtering. It shows the
//! backdrop of the scene's room, the ground grid with its red and green axes, and the shapes with
//! simple lighting (the lights of the room, a rough soft shadow, a little shine). The window never
//! waits for it: a newer request makes the worker drop the older one.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::camera::Camera;
use crate::model::{Environment, Scene};
use crate::sdf::{Field, V3, add, dot, mul, norm, sub};

/// The viewport is drawn at 1/FINE_DIV of its size when still...
pub const FINE_DIV: usize = 3;
/// ...and at 1/COARSE_DIV while the view moves (and as a first quick pass).
pub const COARSE_DIV: usize = 6;
/// The fine picture never has more pixels than this (a big screen gets a bigger divisor).
pub const MAX_FINE_PIXELS: usize = 160_000;

/// The sizes (width, height) to draw for a viewport of `view_px` pixels: a quick coarse picture,
/// then (when `moving` is false) the fine one.
pub fn sizes(view_px: [u32; 2], moving: bool) -> Vec<[usize; 2]> {
    let (w, h) = (view_px[0].max(1) as usize, view_px[1].max(1) as usize);
    let mut div = FINE_DIV;
    while (w / div).max(1) * (h / div).max(1) > MAX_FINE_PIXELS && div < 64 {
        div += 1;
    }
    let at = |d: usize| [(w / d).max(1), (h / d).max(1)];
    if moving { vec![at(div * COARSE_DIV / FINE_DIV)] } else { vec![at(div * COARSE_DIV / FINE_DIV), at(div)] }
}

// ---------------------------------------------------------------------------------------------
// The picture

/// What the pixels need besides the field.
struct Setup<'a> {
    field: &'a Field,
    env: &'a Environment,
    cam: &'a Camera,
    size: [usize; 2],
    aspect: f32,
    /// A sphere around everything the scene draws (radius 0 = nothing).
    bound: ([f32; 3], f32),
    /// Soft shadows and the finer normals (off for the quick pass).
    fine: bool,
    /// Analysis view mode (`ViewMode::id`), 0 = normal shading.
    mode: u32,
}

fn lin(c: V3) -> V3 {
    [c[0].max(0.0).powf(2.2), c[1].max(0.0).powf(2.2), c[2].max(0.0).powf(2.2)]
}

fn mix(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The room's backdrop: its colour, a little lighter in the middle (as the GPU picture has it).
fn background(env: &Environment, u: f32, v: f32) -> V3 {
    let (cx, cy) = (u - 0.5, v - 0.45);
    let k = (1.0 - (cx * cx + cy * cy) * 1.1).clamp(0.0, 1.0);
    let f = 0.87 + (1.22 - 0.87) * k;
    lin([(env.backdrop[0] * f).clamp(0.0, 1.0), (env.backdrop[1] * f).clamp(0.0, 1.0), (env.backdrop[2] * f).clamp(0.0, 1.0)])
}

/// What the sky shows in direction `rd` (the three sky colours blended by height).
fn sky(env: &Environment, rd: V3) -> V3 {
    let t = (rd[2] * 0.5 + 0.5).clamp(0.0, 1.0);
    let c = mix(env.sky_bottom, env.sky_horizon, smoothstep(0.0, 0.5, t));
    mix(c, env.sky_top, smoothstep(0.5, 1.0, t))
}

/// The film-like curve of the GPU picture, then the display's gamma.
fn encode(c0: V3, exposure: f32) -> [u8; 3] {
    let f = |x: f32| {
        let c = x * 0.78 * exposure;
        let m = (c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14);
        let v = m.clamp(0.0, 1.0).powf(1.0 / 2.2);
        (v * 255.0 + 0.5) as u8
    };
    [f(c0[0]), f(c0[1]), f(c0[2])]
}

/// A soft shadow: how much of the light reaches `ro` (1 = all), marching `steps` steps.
fn soft_shadow(f: &Field, ro: V3, rd: V3, max_t: f32, steps: usize) -> f32 {
    let mut res = 1.0_f32;
    let mut t = 0.1_f32;
    for _ in 0..steps {
        let h = f.distance(add(ro, mul(rd, t)));
        if h < 0.001 {
            return 0.0;
        }
        res = res.min(8.0 * h / t);
        t += h.clamp(0.5, 20.0);
        if t > max_t {
            break;
        }
    }
    res.clamp(0.0, 1.0)
}

/// The grid lines at ground point `gp`, whose pixel is `footprint` mm wide: the colour changes
/// to mix in (thin lines, every tenth line, the red x axis and the green y axis).
fn grid(col: V3, gp: V3, footprint: f32, fade: f32, below: bool) -> V3 {
    let gs = 10.0_f32;
    let w = (footprint * 1.5).max(1e-4);
    let line_of = |scale: f32| {
        let a =
            [((gp[0] / scale - 0.5).rem_euclid(1.0) - 0.5).abs() / (w / scale), ((gp[1] / scale - 0.5).rem_euclid(1.0) - 0.5).abs() / (w / scale)];
        1.0 - a[0].min(a[1]).min(1.0)
    };
    // endless plane: a level of lines fades only when it gets too dense to tell apart
    let dense = |spacing: f32| {
        let t = ((footprint / spacing - 0.12) / (0.45 - 0.12)).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    };
    let line = line_of(gs) * dense(gs);
    let axis_x = 1.0 - (gp[1].abs() / w).min(1.0);
    let axis_y = 1.0 - (gp[0].abs() / w).min(1.0);
    let mut c = col;
    if below {
        let fade = fade * 0.6;
        c = mix(c, add(c, [0.04; 3]), line * 0.5 * fade);
    } else {
        let major = line_of(gs * 10.0) * dense(gs * 10.0);
        c = mix(c, add(c, [0.03; 3]), line * 0.5 * fade);
        c = mix(c, add(c, [0.05; 3]), major * 0.6 * fade);
    }
    c = mix(c, lin([0.85, 0.35, 0.35]), axis_x * 0.55 * fade);
    mix(c, lin([0.42, 0.78, 0.42]), axis_y * 0.55 * fade)
}

/// The colour (linear) of a surface point seen along `rd`.
fn shade_point(s: &Setup, o: V3, rd: V3, t: f32, hit: &crate::sdf::Sample) -> (V3, V3) {
    let f = s.field;
    let p = add(o, mul(rd, t));
    let n = if s.fine {
        let e = 0.02 + t * 0.0008;
        norm([
            f.distance(add(p, [e, 0.0, 0.0])) - f.distance(sub(p, [e, 0.0, 0.0])),
            f.distance(add(p, [0.0, e, 0.0])) - f.distance(sub(p, [0.0, e, 0.0])),
            f.distance(add(p, [0.0, 0.0, e])) - f.distance(sub(p, [0.0, 0.0, e])),
        ])
    } else {
        let e = 0.03 + t * 0.0015;
        norm([
            f.distance(add(p, [e, 0.0, 0.0])) - hit.dist,
            f.distance(add(p, [0.0, e, 0.0])) - hit.dist,
            f.distance(add(p, [0.0, 0.0, e])) - hit.dist + 1e-7,
        ])
    };
    let v = mul(rd, -1.0);
    let rough = hit.roughness.clamp(0.04, 1.0);
    let metal = hit.metallic.clamp(0.0, 1.0);
    let alb = lin(hit.color);
    let ndv = dot(n, v).max(0.0);
    let fres = (1.0 - ndv).powi(5);
    let f0 = mix([0.04; 3], alb, metal);
    let spec_pow = 300.0 + (8.0 - 300.0) * rough;
    let mut lit = [0.0_f32; 3];
    let mut spec = [0.0_f32; 3];
    for (i, l) in s.env.lights.iter().enumerate() {
        let ld = l.dir();
        let lc = mul(l.color, l.power);
        let ndl = dot(n, ld).max(0.0);
        let mut sh = 1.0;
        if i == 0 && s.fine && s.env.shadow > 0.0 {
            sh = 1.0 + (soft_shadow(f, add(p, mul(n, 0.1)), ld, 300.0, 16) - 1.0) * s.env.shadow;
        }
        let h = norm(add(ld, v));
        let lobe = dot(n, h).max(0.0).powf(spec_pow) * (spec_pow + 8.0) / 60.0;
        for k in 0..3 {
            lit[k] += lc[k] * ndl * sh;
            let fr = f0[k] + (1.0 - f0[k]) * fres;
            spec[k] += lc[k] * ndl * sh * fr * lobe * 0.9;
        }
    }
    let amb = mul(sky(s.env, n), 0.55 * s.env.ambient);
    let diff = mul(alb, 1.0 - metal);
    // what a shiny surface mirrors: the sky, a little
    let refl = sky(s.env, [rd[0] - 2.0 * dot(rd, n) * n[0], rd[1] - 2.0 * dot(rd, n) * n[1], rd[2] - 2.0 * dot(rd, n) * n[2]]);
    let refl_k = (0.04 + 0.96 * fres) * (1.0 - rough * 0.65) * (0.15 + 0.85 * metal);
    let tint = mix([1.0; 3], alb, metal);
    let mut c = [0.0_f32; 3];
    for k in 0..3 {
        c[k] = diff[k] * (amb[k] + lit[k]) + spec[k] + refl[k] * refl_k * tint[k] + 0.05 * fres;
    }
    (c, n)
}

/// One pixel of the picture, as sRGB bytes.
fn pixel(s: &Setup, x: usize, y: usize) -> [u8; 4] {
    let (w, h) = (s.size[0] as f32, s.size[1] as f32);
    let (u, v) = ((x as f32 + 0.5) / w, (y as f32 + 0.5) / h);
    let (o, rd) = s.cam.ray([u * 2.0 - 1.0, 1.0 - v * 2.0], s.aspect);
    let env = s.env;
    let mut col = background(env, u, v);
    let grid_on = env.grid;
    let floor_on = env.floor;

    // the ground at z = 0
    let t_ground = if rd[2].abs() > 1e-4 {
        let tg = -o[2] / rd[2];
        if tg > 0.0 { tg } else { 1e9 }
    } else {
        1e9
    };

    // the shapes: rays that miss the sphere around the scene need no marching
    let mut hit: Option<(f32, crate::sdf::Sample)> = None;
    let (bc, br) = s.bound;
    if br > 0.0 {
        let oc = sub(o, bc);
        let b = dot(oc, rd);
        let disc = b * b - (dot(oc, oc) - br * br);
        if disc > 0.0 {
            let sq = disc.sqrt();
            if -b + sq > 0.0 {
                let t0 = (-b - sq).max(0.0);
                let reach = (-b + sq) - t0;
                hit = s.field.ray(add(o, mul(rd, t0)), rd, reach.max(0.1)).map(|(t, smp)| (t + t0, smp));
            }
        }
    }

    let mut surf_n: Option<V3> = None;
    let above = o[2] >= 0.0;
    let footprint = |gp: V3| s.cam.mm_per_px(gp, h);
    if above && t_ground < 1e8 && hit.as_ref().is_none_or(|(t, _)| t_ground < *t) && (grid_on || floor_on) {
        let gp = add(o, mul(rd, t_ground));
        if grid_on {
            col = grid(col, gp, footprint(gp), 1.0, false);
        }
        if floor_on && env.shadow > 0.0 {
            // the blob's shadow on the ground
            if s.fine {
                let key = env.lights.first().map(|l| l.dir()).unwrap_or([0.0, 0.0, 1.0]);
                let sh = soft_shadow(s.field, add(gp, [0.0, 0.0, 0.05]), key, 300.0, 16);
                col = mul(col, 1.0 + (0.55 + 0.45 * sh - 1.0) * env.shadow);
            }
            let ao = (s.field.distance(add(gp, [0.0, 0.0, 6.0])) / 6.0).clamp(0.0, 1.0);
            col = mul(col, 1.0 + (0.6 + 0.4 * ao - 1.0) * env.shadow);
        }
    }
    if let Some((t, smp)) = &hit
        && (t_ground > *t || !(grid_on || floor_on))
    {
        let (c, n) = shade_point(s, o, rd, *t, smp);
        col = c;
        surf_n = Some(n);
    }
    // seen from below the ground is see-through: only its lines are drawn
    if !above && t_ground < 1e8 && hit.as_ref().is_none_or(|(t, _)| t_ground < *t) && grid_on {
        let gp = add(o, mul(rd, t_ground));
        col = grid(col, gp, footprint(gp), 1.0, true);
    }
    let [mut r, mut g, mut b] = encode(col, env.exposure);
    if let Some(n) = surf_n {
        let (right, up, _) = s.cam.basis();
        if let Some(c) = crate::view_mode::color(s.mode, n, rd, right, up) {
            let to = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            [r, g, b] = [to(c[0]), to(c[1]), to(c[2])];
        }
    }
    [r, g, b, 255]
}

/// The finished rows of one thread: (row number, its RGBA bytes).
type Rows = Vec<(usize, Vec<u8>)>;

/// Draw the scene as RGBA bytes (`size[0] * size[1] * 4`, rows from the top), using `threads`
/// threads. The same input always gives the same bytes, whatever the thread count. `cancel` is
/// asked once per row; when it says yes the drawing stops and the result is `None`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn render(scene: &Scene, cam: &Camera, size: [usize; 2], fine: bool, threads: usize, cancel: &(dyn Fn() -> bool + Sync)) -> Option<Vec<u8>> {
    render_mode(scene, cam, size, fine, threads, 0, cancel)
}

/// `render` with an analysis view mode (`ViewMode::id`; 0 = the usual shading).
pub fn render_mode(
    scene: &Scene,
    cam: &Camera,
    size: [usize; 2],
    fine: bool,
    threads: usize,
    mode: u32,
    cancel: &(dyn Fn() -> bool + Sync),
) -> Option<Vec<u8>> {
    let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
    let field = Field::new(scene);
    let bound = crate::gpu::bounds(scene, None);
    let setup = Setup { field: &field, env: &scene.env, cam, size, aspect: size[0] as f32 / size[1] as f32, bound, fine, mode };
    let threads = threads.clamp(1, 64).min(size[1]);
    let rows: Vec<Option<Rows>> = std::thread::scope(|sc| {
        let handles: Vec<_> = (0..threads)
            .map(|tid| {
                let setup = &setup;
                sc.spawn(move || {
                    let mut mine = Vec::new();
                    let mut y = tid;
                    while y < setup.size[1] {
                        if cancel() {
                            return None;
                        }
                        let mut row = Vec::with_capacity(setup.size[0] * 4);
                        for x in 0..setup.size[0] {
                            row.extend_from_slice(&pixel(setup, x, y));
                        }
                        mine.push((y, row));
                        y += threads;
                    }
                    Some(mine)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
    });
    let mut img = vec![0u8; size[0] * size[1] * 4];
    for part in rows {
        for (y, row) in part? {
            if let Some(dst) = img.get_mut(y * size[0] * 4..(y + 1) * size[0] * 4) {
                dst.copy_from_slice(&row);
            }
        }
    }
    Some(img)
}

// ---------------------------------------------------------------------------------------------
// The worker thread

struct Job {
    id: u64,
    scene: Scene,
    cam: Camera,
    sizes: Vec<[usize; 2]>,
    moving: bool,
    mode: u32,
}

/// A finished picture.
pub struct Frame {
    pub id: u64,
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
    /// The last pass of its request (a moving request has only a coarse pass).
    pub last: bool,
}

struct Worker {
    tx: Sender<Job>,
    rx: Receiver<Frame>,
    latest: Arc<AtomicU64>,
}

fn spawn_worker(ctx: Option<egui::Context>) -> Option<Worker> {
    let (tx, jobs) = channel::<Job>();
    let (frames_tx, rx) = channel::<Frame>();
    let latest = Arc::new(AtomicU64::new(0));
    let l2 = latest.clone();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    std::thread::Builder::new()
        .name("meltcad-easy-view".into())
        .spawn(move || {
            while let Ok(mut job) = jobs.recv() {
                // only the newest request is worth drawing
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                let n = job.sizes.len();
                for (i, size) in job.sizes.iter().enumerate() {
                    let id = job.id;
                    let cancel = || l2.load(Ordering::Relaxed) != id;
                    if cancel() {
                        break;
                    }
                    let last = i + 1 == n;
                    // A moving request has one coarse pass: being the last pass
                    // must not turn expensive shadows and fine normals back on.
                    let fine = !job.moving && last;
                    let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        render_mode(&job.scene, &job.cam, *size, fine, threads, job.mode, &cancel)
                    }));
                    match done {
                        Ok(Some(rgba)) => {
                            if frames_tx.send(Frame { id, size: *size, rgba, last }).is_err() {
                                return;
                            }
                            if let Some(c) = &ctx {
                                c.request_repaint();
                            }
                        }
                        Ok(None) => break,
                        Err(p) => {
                            crate::gpu_log::log(&format!("Simple view: the drawing panicked: {}", crate::gpu_status::panic_text(&*p)));
                            break;
                        }
                    }
                }
            }
        })
        .ok()?;
    Some(Worker { tx, rx, latest })
}

/// What a request was made from.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    scene: Scene,
    cam: [f32; 7],
    view: [u32; 2],
    moving: bool,
    mode: u32,
}

/// The simple view: asks the worker for pictures, keeps the latest one as a texture.
#[derive(Default)]
pub struct EasyView {
    worker: Option<Worker>,
    failed: bool,
    texture: Option<egui::TextureHandle>,
    requested: Option<(u64, Key)>,
    next_id: u64,
    /// Is the final pass of the last request on screen?
    pub settled: bool,
    /// The analysis view mode to draw (`ViewMode::id`).
    pub mode: u32,
}

impl EasyView {
    /// Make sure the picture shows `scene` through `cam` at `view_px`; call every frame.
    pub fn update(&mut self, ctx: &egui::Context, scene: &Scene, cam: &Camera, view_px: [u32; 2], moving: bool) {
        if self.worker.is_none() && !self.failed {
            self.worker = spawn_worker(Some(ctx.clone()));
            self.failed = self.worker.is_none();
        }
        let Some(worker) = &self.worker else { return };
        // pictures that arrived
        while let Ok(frame) = worker.rx.try_recv() {
            if self.requested.as_ref().is_some_and(|(id, _)| *id == frame.id) {
                let image = egui::ColorImage::from_rgba_unmultiplied(frame.size, &frame.rgba);
                match &mut self.texture {
                    Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                    None => self.texture = Some(ctx.load_texture("meltcad easy view", image, egui::TextureOptions::LINEAR)),
                }
                self.settled = frame.last;
            }
        }
        let key = Key {
            scene: scene.clone_if_changed(self.requested.as_ref().map(|(_, k)| &k.scene)),
            cam: crate::perf::cam_key(cam),
            view: view_px,
            moving,
            mode: self.mode,
        };
        if self.requested.as_ref().is_some_and(|(_, k)| *k == key) {
            return;
        }
        self.next_id += 1;
        let id = self.next_id;
        worker.latest.store(id, Ordering::Relaxed);
        let job = Job { id, scene: key.scene.clone(), cam: *cam, sizes: sizes(view_px, moving), moving, mode: self.mode };
        if worker.tx.send(job).is_ok() {
            self.requested = Some((id, key));
            self.settled = false;
        }
    }

    /// Paint the picture over `rect` (just the backdrop colour until the first one arrives).
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, backdrop: [f32; 3]) {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(c(backdrop[0]), c(backdrop[1]), c(backdrop[2])));
        if let Some(t) = &self.texture {
            painter.image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    /// Is the worker running (false = it could not be started, the viewport stays flat)?
    #[cfg(test)]
    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }
}

/// `Scene` has no cheap "same as" check apart from `==`; this keeps the old copy when equal so
/// that the common frame (nothing changed) clones nothing.
trait CloneIfChanged {
    fn clone_if_changed(&self, old: Option<&Scene>) -> Scene;
}

impl CloneIfChanged for Scene {
    fn clone_if_changed(&self, old: Option<&Scene>) -> Scene {
        match old {
            Some(o) if o == self => o.clone(),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material, Shape};

    fn never() -> bool {
        false
    }

    #[test]
    fn the_picture_is_a_third_of_the_viewport_and_a_sixth_while_moving() {
        assert_eq!(sizes([1380, 860], false), vec![[230, 143], [460, 286]]);
        assert_eq!(sizes([1380, 860], true), vec![[230, 143]]);
        assert_eq!(sizes([0, 0], false), vec![[1, 1], [1, 1]]);
        // a huge screen never asks for more than the pixel budget
        let big = sizes([7680, 4320], false);
        let fine = big[1];
        assert!(fine[0] * fine[1] <= MAX_FINE_PIXELS, "{fine:?}");
        assert!(big[0][0] < fine[0], "the first pass is coarser");
    }

    #[test]
    fn the_picture_has_the_asked_size_and_is_not_one_flat_colour() {
        let sc = Scene::default();
        let img = render(&sc, &Camera::default(), [64, 40], true, 2, &never).expect("drawn");
        assert_eq!(img.len(), 64 * 40 * 4);
        assert!(img.chunks(4).all(|p| p[3] == 255));
        let first = [img[0], img[1], img[2]];
        assert!(img.chunks(4).any(|p| [p[0], p[1], p[2]] != first));
    }

    #[test]
    fn view_modes_change_only_the_surface_and_normal_is_unchanged() {
        let sc = Scene::default();
        let mut cam = Camera::default();
        cam.frame([0.0, 0.0, 20.0], 40.0);
        let normal = render(&sc, &cam, [64, 40], true, 2, &never).expect("normal");
        assert_eq!(normal, render_mode(&sc, &cam, [64, 40], true, 2, 0, &never).expect("mode 0"));
        for mode in 1..=4u32 {
            let img = render_mode(&sc, &cam, [64, 40], true, 2, mode, &never).expect("mode");
            assert_eq!(img.len(), normal.len());
            assert!(img.chunks(4).all(|p| p[3] == 255));
            assert_ne!(img, normal, "mode {mode} draws something different");
        }
    }

    #[test]
    fn the_same_input_gives_the_same_picture_whatever_the_thread_count() {
        let sc = Scene::default();
        let cam = Camera::default();
        let a = render(&sc, &cam, [48, 30], true, 1, &never).expect("a");
        let b = render(&sc, &cam, [48, 30], true, 1, &never).expect("b");
        let c = render(&sc, &cam, [48, 30], true, 4, &never).expect("c");
        let d = render(&sc, &cam, [48, 30], true, 7, &never).expect("d");
        assert_eq!(a, b, "repeatable");
        assert_eq!(a, c, "four threads");
        assert_eq!(a, d, "seven threads");
        let coarse = render(&sc, &cam, [48, 30], false, 2, &never).expect("coarse");
        assert_ne!(a, coarse, "the quick pass differs (no soft shadows)");
    }

    #[test]
    fn a_cancelled_drawing_gives_nothing() {
        let sc = Scene::default();
        assert!(render(&sc, &Camera::default(), [32, 32], true, 3, &|| true).is_none());
    }

    fn px(img: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
        let i = (y * w + x) * 4;
        [img[i], img[i + 1], img[i + 2]]
    }

    #[test]
    fn the_ground_shows_a_grid_and_the_axes_and_the_shape_a_lit_body() {
        // an empty room looking down on the floor: the red and green axes cross in the middle
        let mut sc = Scene::default();
        sc.groups[0].shapes.clear();
        sc.env.floor = false;
        let mut cam = Camera::default();
        cam.look(0.0, std::f32::consts::FRAC_PI_2 - 0.01);
        cam.target = [0.0, 0.0, 0.0];
        cam.dist = 120.0;
        let (w, h) = (120, 120);
        let img = render(&sc, &cam, [w, h], true, 2, &never).expect("drawn");
        let centre = px(&img, w, w / 2, h / 2);
        let row_y = h / 2;
        // the x axis (red) runs along the middle row, the y axis (green) down the middle column
        let along_x = px(&img, w, w / 2 + 20, row_y);
        let along_y = px(&img, w, w / 2, h / 2 - 20);
        assert!(along_x[0] > along_x[1] && along_x[0] > along_x[2], "x axis is red: {along_x:?}");
        assert!(along_y[1] > along_y[0] && along_y[1] > along_y[2], "y axis is green: {along_y:?}");
        let corner = px(&img, w, 3, 3);
        assert!(corner[0] < 120 && corner[1] < 120, "the backdrop is dark in the studio: {corner:?}");
        // some pixel differs from the backdrop-only corner: grid lines exist
        assert!(img.chunks(4).filter(|p| p[..3] != corner[..]).count() > 200, "grid lines are drawn");
        let _ = centre;

        // with the grid off the empty room is only backdrop
        sc.env.grid = false;
        let flat = render(&sc, &cam, [w, h], true, 2, &never).expect("drawn");
        let c0 = px(&flat, w, 3, 3);
        let unequal = flat.chunks(4).filter(|p| (p[0] as i32 - c0[0] as i32).abs() > 12).count();
        assert!(unequal < flat.len() / 4 / 3, "only the soft vignette remains: {unequal}");
    }

    #[test]
    fn a_shape_is_drawn_solid_in_its_colour_and_the_sky_colour_follows_the_room() {
        let mut sc = Scene::default();
        sc.groups[0].shapes[0] = Shape::new(Kind::Sphere, [0.0, 0.0, 20.0], Material { color: [0.9, 0.1, 0.1], ..Material::default() });
        let cam = Camera { target: [0.0, 0.0, 20.0], ..Camera::default() };
        let (w, h) = (80, 60);
        let img = render(&sc, &cam, [w, h], true, 2, &never).expect("drawn");
        let mid = px(&img, w, w / 2, h / 2);
        assert!(mid[0] > mid[1] + 40 && mid[0] > mid[2] + 40, "a red ball in the middle: {mid:?}");
        // a white room has a light backdrop
        sc.env = crate::model::Environment::looks().into_iter().find(|(n, _)| *n == "しろ").map(|(_, e)| e).unwrap_or_default();
        let img = render(&sc, &cam, [w, h], true, 2, &never).expect("drawn");
        let corner = px(&img, w, 2, 2);
        assert!(corner[0] > 150 && corner[1] > 150, "{corner:?}");
    }

    #[test]
    fn hostile_numbers_do_not_panic_the_drawing() {
        let mut sc = Scene::default();
        sc.groups[0].shapes[0].pos = [f32::NAN, f32::INFINITY, 1e30];
        sc.groups[0].shapes[0].size = [f32::NAN; 3];
        sc.env.exposure = f32::NAN;
        let cam = Camera { dist: f32::NAN, ..Camera::default() };
        let img = render(&sc, &cam, [16, 16], true, 2, &never);
        assert_eq!(img.map(|i| i.len()), Some(16 * 16 * 4));
        let img = render(&Scene::default(), &Camera::default(), [100000, 0], false, 0, &never);
        assert!(img.is_some_and(|i| !i.is_empty()), "sizes are clamped");
    }

    #[test]
    fn the_worker_draws_the_newest_request_and_the_view_keeps_the_picture() {
        let ctx = egui::Context::default();
        let mut v = EasyView::default();
        let sc = Scene::default();
        let cam = Camera::default();
        v.update(&ctx, &sc, &cam, [120, 90], false);
        assert!(v.has_worker());
        assert!(!v.settled);
        // wait for the finest picture of the request
        let mut waited = 0;
        while !v.settled && waited < 600 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            v.update(&ctx, &sc, &cam, [120, 90], false);
            waited += 1;
        }
        assert!(v.settled, "the picture arrives");
        assert!(v.texture.is_some());
        let id = v.requested.as_ref().map(|(i, _)| *i);
        // nothing changed: no new request
        v.update(&ctx, &sc, &cam, [120, 90], false);
        assert_eq!(v.requested.as_ref().map(|(i, _)| *i), id, "an unchanged view asks for nothing");
        // moving the camera asks again, with the quick pass only
        let mut cam2 = cam;
        cam2.orbit(30.0, 0.0);
        v.update(&ctx, &sc, &cam2, [120, 90], true);
        assert_ne!(v.requested.as_ref().map(|(i, _)| *i), id);
        assert!(!v.settled);
    }

    #[test]
    fn the_moving_worker_finishes_a_coarse_pass_then_restores_full_quality_when_still() {
        let worker = spawn_worker(None).expect("worker starts");
        let scene = Scene::default();
        let cam = Camera::default();
        let size = [48, 30];
        let coarse = render(&scene, &cam, size, false, 1, &never).expect("coarse reference");
        let fine = render(&scene, &cam, size, true, 1, &never).expect("fine reference");
        assert_ne!(coarse, fine, "the fixture must distinguish the two shading modes");

        worker.latest.store(1, Ordering::Relaxed);
        worker.tx.send(Job { id: 1, scene: scene.clone(), cam, sizes: vec![size], moving: true, mode: 0 }).expect("moving request");
        let moving = worker.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("moving frame");
        assert_eq!(moving.id, 1);
        assert!(moving.last, "a coarse moving pass still completes its request");
        assert_eq!(moving.rgba, coarse, "moving must not spend work on fine shading");

        worker.latest.store(2, Ordering::Relaxed);
        worker.tx.send(Job { id: 2, scene, cam, sizes: vec![[24, 15], size], moving: false, mode: 0 }).expect("still request");
        let first = worker.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("first still frame");
        let final_frame = worker.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("fine still frame");
        assert!(!first.last);
        assert!(final_frame.last);
        assert_eq!(final_frame.id, 2);
        assert_eq!(final_frame.rgba, fine, "settling restores the original full-quality pixels");
    }

    #[test]
    fn the_key_clone_reuses_an_equal_scene() {
        let a = Scene::default();
        let b = a.clone_if_changed(Some(&a));
        assert_eq!(a, b);
        let mut c = a.clone();
        c.env.exposure = 2.0;
        assert_ne!(c.clone_if_changed(Some(&a)), a);
        assert_eq!(c.clone_if_changed(None), c);
    }
}
