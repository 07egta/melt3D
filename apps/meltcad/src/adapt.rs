//! Holding the frame rate: pure controllers (no graphics card needed to test them).
//!
//! * `ScaleController`: while the view or a handle moves, the picture is drawn at a fraction of the
//!   window's pixels. The fraction follows the measured cost of the picture (GPU time from timestamp
//!   queries, or else the time between frames) so that one frame stays inside a target. It drops
//!   quickly when a frame is too slow, climbs slowly, and waits after a drop, so it does not
//!   swing up and down.
//! * `BandPacer`: the sharp picture of a still scene is drawn a band of rows at a time, so that no
//!   single frame has to carry the whole cost of a full-quality march (on a weak graphics card that
//!   is hundreds of milliseconds, a visible stall). The pacer says how many rows fit in a frame.

/// The smallest share of the window's pixels along each side that is drawn while moving.
pub const MIN_SCALE: f32 = 0.25;
/// What one scene pass may cost on the graphics card while moving (ms): about 100 frames a second
/// with room for the rest of the window.
pub const GPU_TARGET_MS: f32 = 10.0;
/// What one frame may take from start to start when only the frame interval is known (ms): 60 a second.
pub const FRAME_TARGET_MS: f32 = 17.5;
/// The most one sharp band may cost (ms).
pub const BAND_BUDGET_MS: f32 = 6.0;

/// One measurement of what a moving picture cost.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sample {
    /// The scene pass took `ms` on the graphics card at this render scale.
    Gpu { ms: f32, scale: f32 },
    /// A whole frame took `ms` (start to start) at this render scale; `cpu_ms` of it was spent on
    /// the processor (a frame that is slow because of the processor says nothing about the picture size).
    Frame { ms: f32, scale: f32, cpu_ms: f32 },
}

/// Picks the render scale of the moving picture.
#[derive(Clone, Debug)]
pub struct ScaleController {
    scale: f32,
    /// Smoothed cost in ms of a picture at scale 1 (None until the first measurement).
    cost: Option<f32>,
    /// Measurements to wait before the scale may rise again.
    hold: u32,
    /// How long the next wait is: triples when a drop follows a rise soon after.
    penalty: u32,
    /// Measurements since the last drop.
    since_drop: u32,
}

impl Default for ScaleController {
    fn default() -> Self {
        ScaleController { scale: 0.6, cost: None, hold: 0, penalty: 12, since_drop: 10_000 }
    }
}

impl ScaleController {
    /// The scale for the next moving picture, `MIN_SCALE..=1`.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Forget what was measured (another scene, another window size).
    #[cfg(test)]
    pub fn reset(&mut self) {
        *self = ScaleController::default();
    }

    /// Take one measurement of a moving picture.
    pub fn observe(&mut self, sample: Sample) {
        let (ms, used, target) = match sample {
            Sample::Gpu { ms, scale } => (ms, scale, GPU_TARGET_MS),
            Sample::Frame { ms, scale, cpu_ms } => {
                // slow because of the processor: shrinking the picture would not help
                if cpu_ms.is_finite() && cpu_ms > 0.7 * ms {
                    return;
                }
                (ms, scale, FRAME_TARGET_MS)
            }
        };
        if !ms.is_finite() || !used.is_finite() || ms <= 0.0 || used < 0.05 {
            return;
        }
        let used = used.min(1.0);
        let c = ms / (used * used);
        // quick to believe a rise in cost, slow to believe a fall
        let cost = match self.cost {
            None => c,
            Some(old) if c > old => old + (c - old) * 0.6,
            Some(old) => old + (c - old) * 0.2,
        };
        self.cost = Some(cost);
        self.since_drop = self.since_drop.saturating_add(1);
        self.hold = self.hold.saturating_sub(1);
        let want = (target / cost).sqrt().clamp(MIN_SCALE, 1.0);
        if want < self.scale * 0.95 || (want <= MIN_SCALE && self.scale > MIN_SCALE) {
            // too slow: down at once (at most a quarter in one step)
            self.scale = want.max(self.scale * 0.75).max(MIN_SCALE);
            // a drop soon after a rise: the rise was too eager, wait longer next time
            self.penalty = if self.since_drop < 90 { (self.penalty * 3).min(300) } else { 12 };
            self.hold = self.penalty;
            self.since_drop = 0;
        } else if (want > self.scale * 1.08 || (want >= 0.999 && self.scale < 1.0)) && self.hold == 0 {
            // room to spare: up slowly
            self.scale = (self.scale * 1.03).min(want).min(1.0);
        }
    }
}

/// How many rows of the sharp picture to draw this frame.
#[derive(Clone, Debug)]
pub struct BandPacer {
    rows: f32,
}

impl Default for BandPacer {
    fn default() -> Self {
        BandPacer { rows: 24.0 }
    }
}

impl BandPacer {
    /// Rows for the next band of a picture `height` rows tall: even (the shader takes derivatives
    /// over 2x2 pixels), at least 8, at most the picture.
    pub fn rows(&self, height: u32) -> u32 {
        let r = if self.rows.is_finite() { self.rows.round().max(8.0) } else { 8.0 };
        let r = (r as u32).min(height.max(1));
        let even = r & !1;
        even.max(2).min(height.max(2))
    }

    /// A band of `rows` rows took `ms` on the graphics card.
    pub fn observe_gpu(&mut self, rows: u32, ms: f32) {
        if rows == 0 || !ms.is_finite() || ms <= 0.0 {
            return;
        }
        let fit = rows as f32 * BAND_BUDGET_MS / ms;
        // down at once, up by at most half again
        self.rows = if fit < self.rows { fit.max(8.0) } else { fit.min(self.rows * 1.5) };
    }

    /// Without timestamps: the frame that drew a band took `ms` from start to start.
    pub fn observe_frame(&mut self, ms: f32) {
        if !ms.is_finite() || ms <= 0.0 {
            return;
        }
        if ms > FRAME_TARGET_MS * 1.3 {
            self.rows = (self.rows * 0.6).max(8.0);
        } else if ms < FRAME_TARGET_MS * 0.9 {
            self.rows = (self.rows * 1.25).min(1.0e5);
        }
    }

    #[cfg(test)]
    pub fn reset(&mut self) {
        *self = BandPacer::default();
    }
}

/// Which rows of the sharp picture are done: bands are drawn top to bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bands {
    pub done: u32,
    pub height: u32,
}

impl Bands {
    pub fn new(height: u32) -> Bands {
        Bands { done: 0, height }
    }

    pub fn complete(&self) -> bool {
        self.done >= self.height
    }

    /// The rows `(first, count)` of the next band of `rows` rows, or None when finished.
    pub fn next(&self, rows: u32) -> Option<(u32, u32)> {
        if self.complete() {
            return None;
        }
        let left = self.height - self.done;
        let count = (rows.max(2) & !1).max(2).min(left);
        // (a tail of one row is taken along)
        let count = if left - count == 1 { left } else { count };
        Some((self.done, count))
    }

    pub fn advance(&mut self, count: u32) {
        self.done = (self.done + count).min(self.height);
    }

    /// The share of the picture (from the top) that is sharp.
    pub fn fraction(&self) -> f32 {
        if self.height == 0 { 1.0 } else { self.done as f32 / self.height as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose cost grows with the pixels: `full` ms at scale 1, with a deterministic wobble.
    fn gpu_ms(full: f32, scale: f32, k: usize) -> f32 {
        let wobble = 1.0 + 0.06 * ((k * 7 % 11) as f32 / 10.0 - 0.5);
        full * scale * scale * wobble
    }

    fn run(c: &mut ScaleController, full: f32, frames: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for k in 0..frames {
            let s = c.scale();
            c.observe(Sample::Gpu { ms: gpu_ms(full, s, k), scale: s });
            out.push(c.scale());
        }
        out
    }

    #[test]
    fn a_heavy_scene_settles_at_the_scale_that_fits() {
        let mut c = ScaleController::default();
        // 60 ms at full size: 10 ms fits at sqrt(10/60) = 0.41
        let s = run(&mut c, 60.0, 200);
        let last = *s.last().unwrap_or(&0.0);
        assert!((last - 0.408).abs() < 0.06, "settled at {last}");
        assert!(s.iter().all(|v| (MIN_SCALE..=1.0).contains(v)));
    }

    #[test]
    fn a_light_scene_goes_to_full_size() {
        let mut c = ScaleController::default();
        let s = run(&mut c, 6.0, 200);
        assert!((s.last().copied().unwrap_or(0.0) - 1.0).abs() < 1e-4, "{:?}", s.last());
    }

    #[test]
    fn an_impossible_scene_stops_at_the_floor() {
        let mut c = ScaleController::default();
        let s = run(&mut c, 2000.0, 200);
        assert!((s.last().copied().unwrap_or(0.0) - MIN_SCALE).abs() < 1e-4);
    }

    #[test]
    fn it_does_not_swing_once_settled() {
        let mut c = ScaleController::default();
        let _ = run(&mut c, 45.0, 150);
        let tail = run(&mut c, 45.0, 400);
        // direction changes of the scale in steady state
        let mut turns = 0;
        let mut dir = 0.0_f32;
        for w in tail.windows(2) {
            let d = w[1] - w[0];
            if d.abs() > 1e-6 {
                if dir != 0.0 && d.signum() != dir.signum() {
                    turns += 1;
                }
                dir = d;
            }
        }
        assert!(turns <= 4, "{turns} direction changes in 400 frames");
        let lo = tail.iter().cloned().fold(f32::MAX, f32::min);
        let hi = tail.iter().cloned().fold(f32::MIN, f32::max);
        assert!(hi - lo < 0.08, "range {lo}..{hi}");
        // and the frames it makes are inside the target (with a little noise)
        let s = c.scale();
        assert!(gpu_ms(45.0, s, 3) < GPU_TARGET_MS * 1.15, "{} ms", gpu_ms(45.0, s, 3));
    }

    #[test]
    fn it_climbs_back_when_the_scene_gets_lighter() {
        let mut c = ScaleController::default();
        let _ = run(&mut c, 100.0, 150);
        let low = c.scale();
        assert!(low < 0.4);
        let s = run(&mut c, 10.0, 300);
        assert!(s.last().copied().unwrap_or(0.0) > 0.95, "{:?}", s.last());
        // slowly: no jump in a single frame
        let mut prev = low;
        for v in &s {
            assert!(*v <= prev * 1.031 + 1e-6, "jumped from {prev} to {v}");
            prev = *v;
        }
    }

    #[test]
    fn it_drops_fast_when_the_scene_gets_heavier() {
        let mut c = ScaleController::default();
        let _ = run(&mut c, 8.0, 100);
        assert!(c.scale() > 0.95);
        let s = run(&mut c, 80.0, 12);
        // within 12 frames it is already close to the fitting 0.35
        assert!(s.last().copied().unwrap_or(1.0) < 0.5, "{:?}", s.last());
    }

    #[test]
    fn a_probe_that_fails_makes_the_next_wait_longer() {
        let mut c = ScaleController::default();
        // a scene that is cheap at first and then pops to expensive as soon as the scale gets above 0.5
        let mut rises_after_drop = Vec::new();
        let mut last_drop = 0usize;
        let mut prev = c.scale();
        for k in 0..600 {
            let s = c.scale();
            let full = if s > 0.55 { 120.0 } else { 20.0 };
            c.observe(Sample::Gpu { ms: gpu_ms(full, s, k), scale: s });
            if c.scale() < prev - 1e-6 {
                last_drop = k;
            } else if c.scale() > prev + 1e-6 && last_drop > 0 {
                rises_after_drop.push(k - last_drop);
            }
            prev = c.scale();
        }
        // the number of probes into the bad region shrinks as the waits get longer
        let first_half = rises_after_drop.iter().filter(|&&d| d < 300).count();
        assert!(first_half > 0);
        let drops = {
            let mut c2 = ScaleController::default();
            let mut n = 0;
            let mut p = c2.scale();
            for k in 0..600 {
                let s = c2.scale();
                let full = if s > 0.55 { 120.0 } else { 20.0 };
                c2.observe(Sample::Gpu { ms: gpu_ms(full, s, k), scale: s });
                if c2.scale() < p - 1e-6 && s > 0.55 {
                    n += 1;
                }
                p = c2.scale();
            }
            n
        };
        assert!(drops <= 6, "{drops} failed probes in 600 frames");
    }

    #[test]
    fn a_slow_processor_does_not_shrink_the_picture() {
        let mut c = ScaleController::default();
        let before = c.scale();
        for _ in 0..50 {
            // 40 ms frames of which 35 ms are the processor's
            c.observe(Sample::Frame { ms: 40.0, scale: before, cpu_ms: 35.0 });
        }
        assert_eq!(c.scale(), before);
        // a slow graphics card does
        for _ in 0..50 {
            let s = c.scale();
            c.observe(Sample::Frame { ms: 80.0 * s * s, scale: s, cpu_ms: 2.0 });
        }
        assert!(c.scale() < before);
    }

    #[test]
    fn frame_intervals_find_a_scale_for_sixty_a_second() {
        let mut c = ScaleController::default();
        for k in 0..400 {
            let s = c.scale();
            // a card that needs 50 ms for a full picture, behind a 60 Hz display
            let ms = (50.0 * s * s * (1.0 + 0.04 * ((k % 5) as f32 - 2.0) / 2.0)).max(16.7);
            c.observe(Sample::Frame { ms, scale: s, cpu_ms: 3.0 });
        }
        let s = c.scale();
        assert!((0.45..=0.75).contains(&s), "{s}");
    }

    #[test]
    fn bad_numbers_change_nothing() {
        let mut c = ScaleController::default();
        let s = c.scale();
        for bad in [f32::NAN, f32::INFINITY, -3.0, 0.0] {
            c.observe(Sample::Gpu { ms: bad, scale: 0.5 });
            c.observe(Sample::Gpu { ms: 5.0, scale: bad });
            c.observe(Sample::Frame { ms: bad, scale: 0.5, cpu_ms: 1.0 });
        }
        assert_eq!(c.scale(), s);
        c.reset();
        assert_eq!(c.scale(), ScaleController::default().scale());
    }

    #[test]
    fn bands_follow_the_cost() {
        let mut p = BandPacer::default();
        // a 24-row band took 12 ms: only 12 rows fit in 6 ms
        p.observe_gpu(24, 12.0);
        assert_eq!(p.rows(1440), 12);
        // cheap bands grow the next one, by at most half again
        let mut p = BandPacer::default();
        p.observe_gpu(24, 0.1);
        assert_eq!(p.rows(1440), 36);
        for _ in 0..20 {
            let r = p.rows(1440);
            p.observe_gpu(r, r as f32 * 0.002);
        }
        assert_eq!(p.rows(1440), 1440, "a tiny scene is drawn in one go");
        assert_eq!(p.rows(100), 100);
        assert_eq!(p.rows(0) % 2, 0);
    }

    #[test]
    fn bands_follow_frame_times_without_timestamps() {
        let mut p = BandPacer::default();
        for _ in 0..6 {
            p.observe_frame(8.0);
        }
        let grown = p.rows(2000);
        assert!(grown > 60, "{grown}");
        p.observe_frame(60.0);
        assert!(p.rows(2000) < grown);
        p.observe_frame(f32::NAN);
        p.observe_gpu(0, 1.0);
        p.observe_gpu(4, f32::NAN);
    }

    #[test]
    fn bands_cover_the_picture_exactly_once() {
        for height in [1u32, 2, 3, 7, 100, 721, 1440] {
            let mut b = Bands::new(height);
            let mut covered = 0u32;
            let mut n = 0;
            while let Some((first, count)) = b.next(37) {
                assert_eq!(first, covered);
                assert!(count >= 1);
                covered += count;
                b.advance(count);
                n += 1;
                assert!(n < 2000);
            }
            assert_eq!(covered, height);
            assert!(b.complete() && (b.fraction() - 1.0).abs() < 1e-6);
        }
        assert_eq!(Bands::new(0).next(10), None);
    }

    #[test]
    fn band_boundaries_stay_even_except_the_tail() {
        let mut b = Bands::new(1441);
        let mut p = BandPacer::default();
        p.observe_gpu(24, 3.0);
        while let Some((first, count)) = b.next(p.rows(1441)) {
            assert_eq!(first % 2, 0, "a band starts on an even row");
            b.advance(count);
        }
    }
}
