//! Keeping MELTCAD light. Pure helpers (so they can be tested without a GPU) for:
//! - the scene revision: the picture is drawn again only when something it shows has changed (see `gpu.rs`);
//! - the fast mode: while the view or a handle is moving, a cheaper picture is drawn, and a full
//!   one follows when things have been still for a moment;
//! - the picture quality choice (render scale);
//! - the hover pick throttle;
//! - bounding spheres for the shader, and the start-up timing printout (`MELTCAD_TIMING=1`).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::camera::Camera;
use crate::model::{Scene, Shape};

// ---------------------------------------------------------------------------------------------
// Start-up timing

static START: OnceLock<Instant> = OnceLock::new();
static TIMING: OnceLock<bool> = OnceLock::new();

/// Start the clock (call first thing in `main`).
pub fn init() {
    let _ = START.set(Instant::now());
}

fn timing_on() -> bool {
    *TIMING.get_or_init(|| std::env::var_os("MELTCAD_TIMING").is_some_and(|v| v != "0" && !v.is_empty()))
}

/// Print `label` with the time since start-up to stderr, when `MELTCAD_TIMING=1`.
pub fn mark(label: &str) {
    if timing_on() {
        let ms = START.get().map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
        eprintln!("[meltcad] {ms:8.1} ms  {label}");
    }
}

/// `mark`, but only the first time `flag` is seen (for per-frame places).
pub fn mark_once(flag: &AtomicBool, label: &str) {
    if timing_on() && !flag.swap(true, Ordering::Relaxed) {
        mark(label);
    }
}

// ---------------------------------------------------------------------------------------------
// Quality

/// How much a picture that is drawn smaller than the window is sharpened when it is scaled up (0..1).
pub const DEFAULT_SHARPEN: f32 = 0.6;

/// How sharp the viewport is drawn (がしつ).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    /// Half size, light on the machine.
    Light,
    /// Three quarters.
    Normal,
    /// Full size always.
    Fine,
    /// Full size when still; while moving, the size that keeps the frame rate (see `adapt::ScaleController`).
    #[default]
    Auto,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Auto, Quality::Light, Quality::Normal, Quality::Fine];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Light => "Light",
            Quality::Normal => "Normal",
            Quality::Fine => "Sharp",
            Quality::Auto => "Auto",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Quality::Light => "Lightest (half the screen detail)",
            Quality::Normal => "Medium (three-quarters detail)",
            Quality::Fine => "Always full detail",
            Quality::Auto => "Sharp when still; lighter while moving to keep up the speed",
        }
    }

    /// The share of the viewport's pixels along each side that is drawn once things are still.
    pub fn still_scale(self) -> f32 {
        self.scale(false)
    }

    /// The share drawn while moving when the choice fixes it (`None`: the controller decides).
    pub fn moving_scale(self) -> Option<f32> {
        match self {
            Quality::Auto => None,
            other => Some(other.scale(true)),
        }
    }

    /// The share of the viewport's pixels along each side that is actually drawn. For `Auto`
    /// while moving this is only the starting guess: the controller picks the real value.
    pub fn scale(self, moving: bool) -> f32 {
        match (self, moving) {
            (Quality::Light, _) => 0.5,
            (Quality::Normal, _) => 0.75,
            (Quality::Fine, false) => 1.0,
            (Quality::Fine, true) => 0.85,
            (Quality::Auto, false) => 1.0,
            (Quality::Auto, true) => 0.5,
        }
    }
}

/// Size of the picture to draw: `viewport` pixels times `scale`, at least 1 and at most `max_dim`.
pub fn render_size(viewport: [u32; 2], scale: f32, max_dim: u32) -> [u32; 2] {
    let s = if scale.is_finite() { scale.clamp(0.1, 1.0) } else { 1.0 };
    let one = |v: u32| (((v as f32) * s).round() as u32).clamp(1, max_dim.max(1));
    [one(viewport[0]), one(viewport[1])]
}

/// The viewport rectangle in whole pixels, rounded the way egui rounds it for paint callbacks.
pub fn viewport_px(rect: egui::Rect, ppp: f32) -> [u32; 2] {
    let edge = |v: f32| (v * ppp).round().max(0.0);
    let w = (edge(rect.max.x) - edge(rect.min.x)).max(0.0);
    let h = (edge(rect.max.y) - edge(rect.min.y)).max(0.0);
    [w as u32, h as u32]
}

// ---------------------------------------------------------------------------------------------
// Activity (fast mode)

/// How long things must stay still before the full picture is drawn.
pub const SETTLE_SECS: f64 = 0.15;

/// When the view or a handle was last moving.
#[derive(Clone, Copy, Debug)]
pub struct Activity {
    last: f64,
}

impl Default for Activity {
    fn default() -> Self {
        Activity { last: f64::NEG_INFINITY }
    }
}

impl Activity {
    /// Something is moving right now.
    pub fn touch(&mut self, now: f64) {
        self.last = now;
    }

    /// Use the existing cheap interactive pass only when the hovered *object* changes.
    /// Mere pointer movement over the same object must not keep full quality deferred.
    pub fn hover_changed(&mut self, now: f64, before: Option<crate::model::Pick>, after: Option<crate::model::Pick>) {
        if before != after {
            self.touch(now);
        }
    }

    /// Draw the cheap picture? True while moving and for `SETTLE_SECS` after.
    pub fn is_fast(&self, now: f64) -> bool {
        now - self.last < SETTLE_SECS
    }

    /// Seconds until the full picture is due (`None` when it already is).
    pub fn settle_in(&self, now: f64) -> Option<f64> {
        let left = self.last + SETTLE_SECS - now;
        (left > 0.0).then_some(left + 0.005)
    }
}

// ---------------------------------------------------------------------------------------------
// Hover pick throttle

/// Least seconds between two hover picks (about 30 per second).
pub const HOVER_GAP: f64 = 1.0 / 30.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gate {
    /// Nothing to do: the mouse has not moved and nothing else changed.
    Same,
    /// Pick now.
    Pick,
    /// Too soon: look again in this many seconds.
    Wait(f64),
}

#[derive(Debug)]
pub struct HoverGate {
    last_pos: Option<[f32; 2]>,
    last_t: f64,
}

impl Default for HoverGate {
    fn default() -> Self {
        HoverGate { last_pos: None, last_t: f64::NEG_INFINITY }
    }
}

impl HoverGate {
    /// `dirty`: the picture under the mouse may have changed (camera or scene) even though the
    /// mouse has not.
    pub fn check(&mut self, pos: [f32; 2], now: f64, dirty: bool) -> Gate {
        let moved = self.last_pos != Some(pos);
        if !moved && !dirty {
            return Gate::Same;
        }
        let wait = self.last_t + HOVER_GAP - now;
        if wait > 0.0 && wait <= HOVER_GAP {
            return Gate::Wait(wait);
        }
        self.last_pos = Some(pos);
        self.last_t = now;
        Gate::Pick
    }

    /// The mouse left or a button went down: pick at once next time.
    pub fn reset(&mut self) {
        self.last_pos = None;
        self.last_t = f64::NEG_INFINITY;
    }
}

// ---------------------------------------------------------------------------------------------
// Camera key

/// The numbers that decide what the camera sees (to tell whether it moved).
pub fn cam_key(c: &Camera) -> [f32; 7] {
    [c.target[0], c.target[1], c.target[2], c.yaw, c.pitch, c.dist, if c.ortho { 1.0 } else { 0.0 }]
}

// ---------------------------------------------------------------------------------------------
// Flags for the shader (in `G.view.w`)

pub const FLAG_SRGB: u32 = 1;
pub const FLAG_FAST: u32 = 2;
pub const FLAG_GLOW: u32 = 4;

/// The value for `G.view.w`.
pub fn view_flags(srgb: bool, fast: bool, glow: bool) -> f32 {
    let mut v = 0;
    if srgb {
        v |= FLAG_SRGB;
    }
    if fast {
        v |= FLAG_FAST;
    }
    if glow {
        v |= FLAG_GLOW;
    }
    v as f32
}

/// Does any *rendered* source contain a visible lamp? A visible linked copy can draw a
/// hidden source, but an unrelated hidden lamp must not force the expensive halo march.
pub fn scene_has_glow(scene: &Scene) -> bool {
    let resolved = crate::pattern::resolve(scene);
    scene_has_glow_resolved(scene, &resolved)
}

/// The same test with the already-resolved groups (packing must not resolve instances twice).
pub fn scene_has_glow_resolved(scene: &Scene, resolved: &[crate::pattern::RGroup]) -> bool {
    resolved.iter().any(|rg| scene.groups.get(rg.src).is_some_and(|source| source.shapes.iter().any(|s| s.visible && s.material.glow > 0.0)))
}

// ---------------------------------------------------------------------------------------------
// Bounding spheres

/// A sphere around the shape (centred on `pos`), a little generous, in mm. The shader skips
/// the shape's exact distance where this says it is far away.
pub fn shape_radius(s: &Shape) -> f32 {
    let h = s.local_half();
    let sc = s.scale;
    let v = [h[0] * sc[0].abs(), h[1] * sc[1].abs(), h[2] * sc[2].abs()];
    let swell = s.face_melt.parts().map_or(0.0, |m| m.2.abs());
    let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() * 1.08 + 1.0 + swell;
    if r.is_finite() { r.min(1.0e4) } else { 1.0e4 }
}

/// Centre and radius of a sphere around the given shapes (radius 0 when there are none).
pub fn sphere_of(scene: &Scene, shapes: &[(usize, usize)]) -> [f32; 4] {
    let list: Vec<&Shape> = shapes.iter().filter_map(|(g, s)| scene.shape(*g, *s)).collect();
    if list.is_empty() {
        return [0.0; 4];
    }
    let n = list.len() as f32;
    let mut c = [0.0_f32; 3];
    for s in &list {
        for (i, v) in c.iter_mut().enumerate() {
            *v += s.pos[i] / n;
        }
    }
    let mut r = 0.0_f32;
    for s in &list {
        let d = [s.pos[0] - c[0], s.pos[1] - c[1], s.pos[2] - c[2]];
        r = r.max((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() + shape_radius(s));
    }
    if !r.is_finite() {
        return [0.0; 4];
    }
    [c[0], c[1], c[2], r + 1.0]
}

// ---------------------------------------------------------------------------------------------
// What the app keeps between frames

/// What the last viewport picture was made from. The window repaints only when something
/// happens, so anything that changes after the picture was made (a floating panel edits the
/// scene, say) must ask for one more frame: that is what `stale` finds out. The scene is
/// represented by its revision number (`App::track_scene`), so nothing is cloned or compared here.
#[derive(Default, Debug)]
pub struct Drawn {
    rev: u64,
    sel: Option<crate::model::Pick>,
    hover: Option<crate::model::Pick>,
    cam: [f32; 7],
    clean: bool,
    stale_run: u32,
}

/// How many frames in a row may be asked for because of a mismatch (a scene that never equals
/// itself, such as one holding NaN, must not keep the window drawing forever).
const MAX_STALE_RUN: u32 = 3;

impl Drawn {
    /// Remember what the picture just made shows.
    pub fn note(&mut self, rev: u64, sel: Option<crate::model::Pick>, hover: Option<crate::model::Pick>, cam: &Camera, clean: bool) {
        self.rev = rev;
        self.sel = sel;
        self.hover = hover;
        self.cam = cam_key(cam);
        self.clean = clean;
    }

    /// Has the scene changed since the picture (while looking at it, the person edited it)?
    pub fn scene_changed(&self, rev: u64) -> bool {
        self.rev != rev
    }

    /// Does the picture differ from what is now on the screen's model? Asks for another frame at
    /// most `MAX_STALE_RUN` times in a row.
    pub fn wants_another_frame(
        &mut self,
        rev: u64,
        sel: Option<crate::model::Pick>,
        hover: Option<crate::model::Pick>,
        cam: &Camera,
        clean: bool,
    ) -> bool {
        let shown_sel = if clean { None } else { sel };
        let shown_hover = if clean { None } else { hover };
        let stale = self.rev != rev || self.sel != shown_sel || self.hover != shown_hover || self.cam != cam_key(cam) || self.clean != clean;
        if !stale {
            self.stale_run = 0;
            return false;
        }
        self.stale_run += 1;
        self.stale_run <= MAX_STALE_RUN
    }
}

/// The scene list packed for the shader, kept while the scene is unchanged: a frame that only
/// moves the camera packs nothing.
#[derive(Default)]
pub struct PackCache {
    rev: Option<u64>,
    pack: Option<std::sync::Arc<crate::gpu::ScenePack>>,
    /// How many times the scene was packed (for tests and the HUD).
    pub packs: u64,
}

impl PackCache {
    /// The pack of `scene` (revision `rev`): the cached one, or a new one.
    pub fn get(&mut self, scene: &Scene, rev: u64) -> std::sync::Arc<crate::gpu::ScenePack> {
        if let (Some(r), Some(p)) = (self.rev, self.pack.as_ref())
            && r == rev
        {
            return p.clone();
        }
        let p = std::sync::Arc::new(crate::gpu::ScenePack::new(scene, crate::gpu::VolumeSet::of(scene)));
        self.rev = Some(rev);
        self.pack = Some(p.clone());
        self.packs += 1;
        p
    }
}

impl std::fmt::Debug for PackCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PackCache({} packs)", self.packs)
    }
}

#[derive(Default, Debug)]
pub struct Perf {
    pub quality: Quality,
    pub activity: Activity,
    pub hover_gate: HoverGate,
    pub drawn: Drawn,
    /// The window title that was last sent (sending it again wakes the event loop).
    pub last_title: String,
    pub last_cam: [f32; 7],
    /// Picks the render scale while things move.
    pub scale: crate::adapt::ScaleController,
    /// Frame times (for the HUD).
    pub stats: crate::hud::FrameStats,
    /// Show the frame-time HUD (F3).
    pub hud: bool,
    /// What the graphics side reports (timings, how much of the sharp picture is done).
    pub feedback: crate::gpu::Feedback,
    pub pack: PackCache,
    /// The scale the last moving picture was asked for, and whether it was a moving picture.
    pub last_scale: f32,
    pub last_fast: bool,
    /// A benchmark is requested from the menu.
    pub bench_requested: bool,
    /// Sharpen picture that is scaled up (0 = off).
    pub sharpen: f32,
}

/// What the 'がしつ' menu shows and changes (copied out of `Perf` while the menu is drawn).
#[derive(Clone, Copy, Debug)]
pub struct MenuState {
    pub quality: Quality,
    pub hud: bool,
    /// Set when "Speed test" was clicked.
    pub bench: bool,
}

/// The 'がしつ' menu of the top bar.
/// `extra` adds entries below the quality choices (the graphics path's: simple view, backend, log).
pub fn quality_menu(ui: &mut egui::Ui, menu: &mut MenuState, extra: impl FnOnce(&mut egui::Ui)) {
    ui.menu_button(egui::RichText::new(format!("Quality: {}", menu.quality.label())).size(13.0), |ui| {
        for q in Quality::ALL {
            ui.selectable_value(&mut menu.quality, q, q.label()).on_hover_text(q.hint());
        }
        ui.separator();
        ui.checkbox(&mut menu.hud, "Show frame time (F3)")
            .on_hover_text("Shows frames per second, CPU and GPU time, and the draw scale over the view");
        if ui.button("Speed test…").on_hover_text("Draws a few scenes and shows this machine's speed in a table").clicked() {
            menu.bench = true;
            ui.close();
        }
        extra(ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material};
    use crate::sdf::Prepared;

    #[test]
    fn fast_mode_lasts_a_moment_after_the_last_move() {
        let mut a = Activity::default();
        assert!(!a.is_fast(0.0), "fresh start is full quality");
        assert_eq!(a.settle_in(0.0), None);
        a.touch(10.0);
        assert!(a.is_fast(10.0));
        assert!(a.is_fast(10.1));
        assert!(!a.is_fast(10.2));
        let left = a.settle_in(10.05).unwrap_or(0.0);
        assert!(left > 0.1 && left < 0.12, "{left}");
        assert_eq!(a.settle_in(10.2), None);
        a.touch(10.15);
        assert!(a.is_fast(10.2), "a new move starts the wait again");
    }

    #[test]
    fn hover_only_enters_fast_mode_when_the_target_changes() {
        use crate::model::Pick;
        let a = Some(Pick::Shape(0, 0));
        let b = Some(Pick::Shape(0, 1));
        let mut activity = Activity::default();
        activity.hover_changed(1.0, None, None);
        assert!(!activity.is_fast(1.0));
        activity.hover_changed(2.0, None, a);
        assert!(activity.is_fast(2.0));
        activity.hover_changed(2.20, a, a);
        assert!(!activity.is_fast(2.20), "moving inside a shape must let the sharp image settle");
        activity.hover_changed(3.0, a, b);
        assert!(activity.is_fast(3.0));
        assert!(!activity.is_fast(3.0 + SETTLE_SECS + 0.01));
        activity.hover_changed(4.0, b, None);
        assert!(activity.is_fast(4.0), "leaving a shape also uses the cheap pass briefly");
    }

    #[test]
    fn quality_scales() {
        assert_eq!(Quality::Light.scale(false), 0.5);
        assert_eq!(Quality::Normal.scale(true), 0.75);
        assert_eq!(Quality::Fine.scale(false), 1.0);
        assert_eq!(Quality::Auto.scale(false), 1.0);
        assert_eq!(Quality::Auto.scale(true), 0.5);
        for q in Quality::ALL {
            for moving in [false, true] {
                let s = q.scale(moving);
                assert!(s > 0.0 && s <= 1.0);
            }
            assert!(q.scale(true) <= q.scale(false), "moving is never sharper");
        }
        assert_eq!(Quality::default(), Quality::Auto);
    }

    #[test]
    fn render_size_is_clamped() {
        assert_eq!(render_size([1000, 500], 0.5, 8192), [500, 250]);
        assert_eq!(render_size([1001, 501], 0.75, 8192), [751, 376]);
        assert_eq!(render_size([0, 0], 1.0, 8192), [1, 1]);
        assert_eq!(render_size([20000, 100], 1.0, 8192), [8192, 100]);
        assert_eq!(render_size([100, 100], f32::NAN, 8192), [100, 100]);
        assert_eq!(render_size([100, 100], 7.0, 8192), [100, 100]);
        assert_eq!(render_size([100, 100], -1.0, 8192), [10, 10]);
    }

    #[test]
    fn viewport_pixels_round_like_egui() {
        let r = egui::Rect::from_min_max(egui::pos2(10.4, 20.6), egui::pos2(110.6, 220.4));
        assert_eq!(viewport_px(r, 1.0), [101, 199]);
        assert_eq!(viewport_px(r, 2.0), [200, 400]);
        assert_eq!(viewport_px(egui::Rect::NOTHING, 1.0), [0, 0]);
    }

    #[test]
    fn hover_picks_only_when_the_mouse_moved_and_not_too_often() {
        let mut g = HoverGate::default();
        assert_eq!(g.check([5.0, 5.0], 1.0, false), Gate::Pick, "first look");
        assert_eq!(g.check([5.0, 5.0], 1.001, false), Gate::Same, "mouse did not move");
        assert_eq!(g.check([5.0, 5.0], 1.5, false), Gate::Same);
        match g.check([6.0, 5.0], 1.51, false) {
            Gate::Pick => {}
            other => panic!("long enough ago: {other:?}"),
        }
        match g.check([7.0, 5.0], 1.52, false) {
            Gate::Wait(w) => assert!(w > 0.0 && w <= HOVER_GAP),
            other => panic!("too soon: {other:?}"),
        }
        assert_eq!(g.check([7.0, 5.0], 1.52 + HOVER_GAP + 0.001, false), Gate::Pick, "the waiting move is picked later");
        assert_eq!(g.check([7.0, 5.0], 3.0, true), Gate::Pick, "camera moved under a still mouse");
        g.reset();
        assert_eq!(g.check([7.0, 5.0], 3.001, false), Gate::Pick, "after a reset the next look is at once");
    }

    #[test]
    fn a_change_after_the_picture_asks_for_one_more_frame() {
        use crate::model::Pick;
        let cam = Camera::default();
        let mut d = Drawn::default();
        d.note(1, Some(Pick::Shape(0, 0)), None, &cam, false);
        assert!(!d.wants_another_frame(1, Some(Pick::Shape(0, 0)), None, &cam, false));
        assert!(d.wants_another_frame(1, None, None, &cam, false), "selection changed");
        d.note(1, None, None, &cam, false);
        assert!(!d.wants_another_frame(1, None, None, &cam, false), "caught up");
        // the scene was edited (its revision moved) later in the frame
        assert!(d.scene_changed(2));
        assert!(d.wants_another_frame(2, None, None, &cam, false), "scene edited later in the frame");
        // a picture without overlays: the selection is not shown
        d.note(2, None, None, &cam, true);
        assert!(!d.wants_another_frame(2, Some(Pick::Shape(0, 0)), Some(Pick::Shape(0, 0)), &cam, true));
        assert!(d.wants_another_frame(2, Some(Pick::Shape(0, 0)), None, &cam, false), "overlays are back");
    }

    #[test]
    fn a_scene_that_never_equals_itself_cannot_loop_forever() {
        // (its revision moves at every look: a scene holding NaN is never equal to its copy)
        let cam = Camera::default();
        let mut d = Drawn::default();
        d.note(1, None, None, &cam, false);
        let asks = (0..10).filter(|k| d.wants_another_frame(2 + *k as u64, None, None, &cam, false)).count();
        assert_eq!(asks, MAX_STALE_RUN as usize);
    }

    #[test]
    fn the_pack_is_made_again_only_when_the_scene_changed() {
        let mut scene = Scene::default();
        let mut c = PackCache::default();
        let a = c.get(&scene, 1);
        let b = c.get(&scene, 1);
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        assert_eq!(c.packs, 1);
        if let Some(sh) = scene.shape_mut(0, 0) {
            sh.pos[0] += 3.0;
        }
        let d = c.get(&scene, 2);
        assert!(!std::sync::Arc::ptr_eq(&a, &d) && d.id != a.id);
        assert_eq!(c.packs, 2);
    }

    /// The shader skips a shape when it is `reach` further than the rest. That is exact only if
    /// the soft minimum then returns the rest unchanged, for every blend mode.
    #[test]
    fn a_far_shape_does_not_change_a_blend() {
        use crate::model::BlendMode;
        // the same numbers as `reach_mult()` in shader.wgsl
        let reach = |m: BlendMode| match m {
            BlendMode::Smooth => 1.0,
            BlendMode::Round => 1.3,
            BlendMode::Gooey => 6.0,
        };
        for mode in [BlendMode::Smooth, BlendMode::Round, BlendMode::Gooey] {
            for k in [0.0_f32, 0.5, 3.0, 12.0, 40.0] {
                for a in [-30.0_f32, -8.0, -0.5, 0.0, 2.0, 30.0] {
                    // the same as `far_enough()` in shader.wgsl
                    let mut need = reach(mode) * k + a + 0.02;
                    if mode == BlendMode::Round {
                        need = need.max(reach(mode) * k + 0.02);
                    }
                    let (d, keep) = crate::sdf::smin(a, need, k, mode);
                    assert!((d - a).abs() < 1e-4, "{mode:?} k {k} a {a}: {d}");
                    assert!(keep > 0.999, "{mode:?} k {k} a {a}: keeps {keep}");
                    // and further away than that
                    let (d, _) = crate::sdf::smin(a, need + 25.0, k, mode);
                    assert!((d - a).abs() < 1e-4, "{mode:?} k {k} a {a}: {d}");
                }
            }
        }
    }

    #[test]
    fn flags_combine() {
        assert_eq!(view_flags(true, false, false), 1.0);
        assert_eq!(view_flags(false, true, false), 2.0);
        assert_eq!(view_flags(true, true, true), 7.0);
        assert_eq!(view_flags(false, false, false), 0.0);
    }

    #[test]
    fn camera_key_changes_with_the_camera() {
        let mut c = Camera::default();
        let k = cam_key(&c);
        assert_eq!(k, cam_key(&c));
        c.orbit(10.0, 0.0);
        assert_ne!(k, cam_key(&c));
    }

    #[test]
    fn glow_is_found() {
        let mut s = Scene::default();
        assert!(!scene_has_glow(&s));
        if let Some(sh) = s.shape_mut(0, 0) {
            sh.material.glow = 0.5;
        }
        assert!(scene_has_glow(&s));
        if let Some(sh) = s.shape_mut(0, 0) {
            sh.visible = false;
        }
        assert!(!scene_has_glow(&s), "a hidden lamp does not count");
        // a copy of a lamp is a lamp, even when the lamp group itself is hidden
        if let Some(sh) = s.shape_mut(0, 0) {
            sh.visible = true;
            sh.material.glow = 0.5;
        }
        let uid = s.ensure_group_uid(0).unwrap_or(0);
        let mut copy = crate::model::Group::new("copy");
        copy.instance_of = Some(uid);
        s.groups.push(copy);
        s.groups[0].visible = false;
        assert!(scene_has_glow(&s));
        assert_eq!(scene_has_glow(&s), scene_has_glow_resolved(&s, &crate::pattern::resolve(&s)));

        // An unrelated hidden lamp must not turn on the 200-step material march
        // just because a different visible group happens to be a linked copy.
        let mut unrelated = Scene::default();
        let uid = unrelated.ensure_group_uid(0).unwrap_or(0);
        let mut lit = crate::model::Group::new("hidden, unused lamp");
        lit.visible = false;
        lit.shapes.push(Shape::new(Kind::Sphere, [300.0, 0.0, 20.0], Material { glow: 3.0, ..Material::default() }));
        unrelated.groups.push(lit);
        let mut linked = crate::model::Group::new("visible ordinary copy");
        linked.instance_of = Some(uid);
        unrelated.groups.push(linked);
        assert!(!scene_has_glow(&unrelated));
        assert!(!crate::gpu::ScenePack::new(&unrelated, crate::gpu::VolumeSet::of(&unrelated)).glow);

        // Broken links have no rendered source, and non-visible shapes cannot emit.
        unrelated.groups[2].instance_of = Some(999_999);
        assert!(!scene_has_glow(&unrelated));
        unrelated.groups[1].visible = true;
        assert!(scene_has_glow(&unrelated), "a directly visible lamp must still emit");
        unrelated.groups[1].visible = false;
        unrelated.groups[2].instance_of = Some(uid);
        unrelated.groups[0].shapes[0].material.glow = 2.0;
        assert!(scene_has_glow(&unrelated));
    }

    /// Outside the bounding sphere, a basic solid is never reached (so the shader may skip it).
    #[test]
    fn the_bounding_sphere_holds_the_basic_solids() {
        let kinds = [Kind::Sphere, Kind::Box, Kind::Cylinder, Kind::Torus, Kind::Cone, Kind::Extrude];
        for kind in kinds {
            for (scale, rot) in [([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]), ([2.0, 0.5, 1.5], [30.0, 40.0, 50.0]), ([0.3, 3.0, 1.0], [10.0, 80.0, 0.0])] {
                let mut sh = Shape::new(kind, [5.0, -3.0, 2.0], Material::default());
                sh.scale = scale;
                sh.rot = rot;
                let r = shape_radius(&sh);
                assert!(r.is_finite() && r > 0.0);
                let p = Prepared::new(&sh);
                // a coarse sphere of directions
                for i in 0..24 {
                    for j in 0..12 {
                        let a = i as f32 / 24.0 * std::f32::consts::TAU;
                        let b = (j as f32 / 11.0 - 0.5) * std::f32::consts::PI;
                        let dir = [a.cos() * b.cos(), a.sin() * b.cos(), b.sin()];
                        let pt = [sh.pos[0] + dir[0] * r, sh.pos[1] + dir[1] * r, sh.pos[2] + dir[2] * r];
                        assert!(p.distance(pt) > 0.0, "{kind:?} scale {scale:?} reaches out of its sphere at {dir:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_selection_sphere_covers_all_picked_shapes() {
        let mut s = Scene::default();
        let mut b = s.groups[0].shapes[0].clone();
        b.pos = [100.0, 0.0, 0.0];
        s.groups[0].shapes.push(b);
        let sp = sphere_of(&s, &[(0, 0), (0, 1)]);
        assert!(sp[3] > 50.0);
        for sh in &s.groups[0].shapes {
            let d = ((sh.pos[0] - sp[0]).powi(2) + (sh.pos[1] - sp[1]).powi(2) + (sh.pos[2] - sp[2]).powi(2)).sqrt();
            assert!(d + shape_radius(sh) <= sp[3]);
        }
        assert_eq!(sphere_of(&s, &[]), [0.0; 4]);
        assert_eq!(sphere_of(&s, &[(5, 5)]), [0.0; 4]);
    }

    #[test]
    fn hostile_sizes_stay_finite() {
        let mut sh = Shape::new(Kind::Sphere, [0.0; 3], Material::default());
        sh.size = [f32::NAN, f32::INFINITY, -1.0];
        sh.scale = [f32::INFINITY, 1.0, 1.0];
        let r = shape_radius(&sh);
        assert!(r.is_finite() && r <= 1.0e4);
    }
}
