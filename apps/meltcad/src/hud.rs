//! The frame-time HUD (F3): how long the frames take, so that a slow machine can be measured and
//! described in numbers. The statistics are plain data (tested without a window); the drawing is a
//! small egui area over the viewport.

use std::collections::VecDeque;

/// How far back the rolling figures look (seconds).
pub const WINDOW_SECS: f64 = 1.0;
/// How far back the "worst frame" figure looks (seconds).
pub const WORST_SECS: f64 = 10.0;
/// A pause longer than this between two frames is the window being idle, not a slow frame.
pub const IDLE_GAP_SECS: f64 = 0.25;

/// What one frame cost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSample {
    /// When the frame started (seconds, any steady clock).
    pub at: f64,
    /// Time from the start of the previous frame (ms); 0 for a frame after an idle pause.
    pub interval_ms: f32,
    /// Time the processor spent making the window's contents (ms).
    pub cpu_ms: f32,
}

/// Rolling frame statistics.
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    frames: VecDeque<FrameSample>,
    gpu: VecDeque<(f64, f32)>,
    last_start: Option<f64>,
    /// Frames counted since the start.
    pub total_frames: u64,
}

/// The figures the HUD shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub fps: f32,
    pub interval_avg_ms: f32,
    pub interval_worst_ms: f32,
    /// The worst interval of the last `WORST_SECS`.
    pub interval_worst_long_ms: f32,
    pub cpu_avg_ms: f32,
    pub cpu_worst_ms: f32,
    pub gpu_avg_ms: Option<f32>,
    pub gpu_worst_ms: Option<f32>,
    /// The last scene pass, when none was timed in the last second (the picture was still).
    pub gpu_last_ms: Option<f32>,
}

impl FrameStats {
    /// A frame starts at `now` (seconds) and the processor needed `cpu_ms` for it.
    #[cfg(test)]
    pub fn frame(&mut self, now: f64, cpu_ms: f32) {
        self.begin(now);
        self.end(cpu_ms);
    }

    /// A frame starts at `now` (seconds): its interval from the frame before is known at once.
    pub fn begin(&mut self, now: f64) {
        let interval_ms = match self.last_start {
            Some(t) if now - t <= IDLE_GAP_SECS && now >= t => ((now - t) * 1000.0) as f32,
            _ => 0.0,
        };
        self.last_start = Some(now);
        self.total_frames += 1;
        self.frames.push_back(FrameSample { at: now, interval_ms, cpu_ms: 0.0 });
        self.trim(now);
    }

    /// The frame that began last is over: the processor needed `cpu_ms` for it.
    pub fn end(&mut self, cpu_ms: f32) {
        if let Some(f) = self.frames.back_mut() {
            f.cpu_ms = if cpu_ms.is_finite() { cpu_ms.max(0.0) } else { 0.0 };
        }
    }

    /// The graphics card took `ms` for a scene pass.
    pub fn gpu(&mut self, now: f64, ms: f32) {
        if ms.is_finite() && ms >= 0.0 {
            self.gpu.push_back((now, ms));
        }
        self.trim(now);
    }

    fn trim(&mut self, now: f64) {
        while self.frames.front().is_some_and(|f| now - f.at > WORST_SECS) {
            self.frames.pop_front();
        }
        while self.gpu.front().is_some_and(|g| now - g.0 > WORST_SECS) {
            self.gpu.pop_front();
        }
    }

    /// The last `WINDOW_SECS` (and the worst of the last `WORST_SECS`).
    pub fn summary(&self, now: f64) -> Summary {
        let recent: Vec<&FrameSample> = self.frames.iter().filter(|f| now - f.at <= WINDOW_SECS).collect();
        let timed: Vec<f32> = recent.iter().map(|f| f.interval_ms).filter(|v| *v > 0.0).collect();
        let avg = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let worst = |v: &[f32]| v.iter().cloned().fold(0.0_f32, f32::max);
        let cpu: Vec<f32> = recent.iter().map(|f| f.cpu_ms).collect();
        let long: Vec<f32> = self.frames.iter().map(|f| f.interval_ms).collect();
        let interval_avg_ms = avg(&timed);
        let gpu: Vec<f32> = self.gpu.iter().filter(|g| now - g.0 <= WINDOW_SECS).map(|g| g.1).collect();
        Summary {
            fps: if interval_avg_ms > 0.0 { 1000.0 / interval_avg_ms } else { 0.0 },
            interval_avg_ms,
            interval_worst_ms: worst(&timed),
            interval_worst_long_ms: worst(&long),
            cpu_avg_ms: avg(&cpu),
            cpu_worst_ms: worst(&cpu),
            gpu_avg_ms: (!gpu.is_empty()).then(|| avg(&gpu)),
            gpu_worst_ms: (!gpu.is_empty()).then(|| worst(&gpu)),
            gpu_last_ms: if gpu.is_empty() { self.gpu.back().map(|g| g.1) } else { None },
        }
    }

    /// The processor time of the frame before the one that is running (ms), 0 when not known.
    pub fn prev_cpu_ms(&self) -> f32 {
        let n = self.frames.len();
        if n < 2 { 0.0 } else { self.frames.get(n - 2).map_or(0.0, |f| f.cpu_ms) }
    }

    /// The most recent frame interval (ms): 0 for the first frame and after an idle pause.
    pub fn last_interval_ms(&self) -> f32 {
        self.frames.back().map_or(0.0, |f| f.interval_ms)
    }
}

/// What the HUD says about the scene and the graphics path besides the times.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub shapes: usize,
    pub instances: usize,
    pub groups: usize,
    /// The render scale of the picture on screen (1 = every pixel).
    pub scale: f32,
    /// "Vulkan / Intel(R) Arc(TM) 140V" or the like.
    pub adapter: String,
    /// Can the graphics card time its own work?
    pub gpu_timing: bool,
    /// The picture is the cheap one (moving) or the sharp one.
    pub fast: bool,
    /// The sharp picture is being drawn band by band: share done, 0..1 (1 = finished).
    pub sharp: f32,
    /// The window's picture size in pixels.
    pub pixels: [u32; 2],
    /// Quality choice shown to the person ("じどう").
    pub quality: String,
}

fn ms(v: f32) -> String {
    if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// The HUD text, one line per entry.
pub fn lines(s: &Summary, c: &Context) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "{:.0} fps　間隔 平均 {} / 最悪 {} ms (10秒で {})",
        s.fps,
        ms(s.interval_avg_ms),
        ms(s.interval_worst_ms),
        ms(s.interval_worst_long_ms)
    ));
    out.push(format!("CPU 平均 {} / 最悪 {} ms", ms(s.cpu_avg_ms), ms(s.cpu_worst_ms)));
    out.push(match (s.gpu_avg_ms, s.gpu_worst_ms) {
        (Some(a), Some(w)) => format!("GPU(シーン) 平均 {} / 最悪 {} ms", ms(a), ms(w)),
        _ if s.gpu_last_ms.is_some() => format!("GPU(シーン) さいごの1回 {} ms", ms(s.gpu_last_ms.unwrap_or(0.0))),
        _ if c.gpu_timing => "GPU(シーン) 計測まち…".to_string(),
        _ => "GPU の時間はこの機械では測れません".to_string(),
    });
    let state = if c.fast {
        "うごき中 (かるい絵)".to_string()
    } else if c.sharp < 1.0 {
        format!("きれいに描画中 {:.0}%", c.sharp * 100.0)
    } else {
        "きれい (止まっている)".to_string()
    };
    out.push(format!("{}　絵 {}x{} 倍率 {:.2}　{}", c.quality, c.pixels[0], c.pixels[1], c.scale, state));
    out.push(format!("形 {}　グループ {}　コピー {}", c.shapes, c.groups, c.instances));
    if !c.adapter.is_empty() {
        out.push(c.adapter.clone());
    }
    out
}

/// Draw the HUD over the viewport `rect`.
pub fn show(ctx: &egui::Context, rect: egui::Rect, summary: &Summary, info: &Context) {
    egui::Area::new(egui::Id::new("frame_hud"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.left_bottom() + egui::vec2(12.0, -12.0))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new().fill(egui::Color32::from_black_alpha(190)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(10, 6)).show(
                ui,
                |ui| {
                    let worst = summary.interval_worst_ms;
                    for (i, l) in lines(summary, info).iter().enumerate() {
                        let color =
                            if i == 0 && worst > 33.0 { egui::Color32::from_rgb(255, 150, 120) } else { egui::Color32::from_rgb(210, 235, 215) };
                        ui.label(egui::RichText::new(l).monospace().size(12.0).color(color));
                    }
                },
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_frames_give_the_frame_rate() {
        let mut s = FrameStats::default();
        for k in 0..120 {
            s.frame(k as f64 * 0.016, 2.0);
        }
        let now = 119.0 * 0.016;
        let sum = s.summary(now);
        assert!((sum.fps - 62.5).abs() < 0.5, "{}", sum.fps);
        assert!((sum.interval_avg_ms - 16.0).abs() < 0.1);
        assert!((sum.interval_worst_ms - 16.0).abs() < 0.1);
        assert!((sum.cpu_avg_ms - 2.0).abs() < 1e-3 && (sum.cpu_worst_ms - 2.0).abs() < 1e-3);
        assert_eq!(s.total_frames, 120);
    }

    #[test]
    fn the_worst_frame_is_remembered_longer_than_the_rolling_figures() {
        let mut s = FrameStats::default();
        let mut t = 0.0;
        for k in 0..400 {
            // one 200 ms hitch at frame 100
            t += if k == 100 { 0.2 } else { 0.016 };
            s.frame(t, 1.0);
        }
        let sum = s.summary(t);
        assert!(sum.interval_worst_ms < 20.0, "the last second is smooth again: {}", sum.interval_worst_ms);
        assert!(sum.interval_worst_long_ms > 190.0, "the last 10 s still know: {}", sum.interval_worst_long_ms);
        // a hitch inside the last second shows
        s.frame(t + 0.12, 1.0);
        assert!(s.summary(t + 0.12).interval_worst_ms > 110.0);
    }

    #[test]
    fn an_idle_pause_is_not_a_slow_frame() {
        let mut s = FrameStats::default();
        s.frame(0.0, 1.0);
        s.frame(0.016, 1.0);
        s.frame(5.0, 1.0);
        s.frame(5.016, 1.0);
        let sum = s.summary(5.016);
        assert!((sum.interval_worst_long_ms - 16.0).abs() < 0.1, "{}", sum.interval_worst_long_ms);
        assert_eq!(s.last_interval_ms().round(), 16.0);
        s.frame(9.0, 1.0);
        assert_eq!(s.last_interval_ms(), 0.0);
    }

    #[test]
    fn old_samples_are_dropped() {
        let mut s = FrameStats::default();
        for k in 0..2000 {
            s.frame(k as f64 * 0.01, 1.0);
            s.gpu(k as f64 * 0.01, 3.0);
        }
        assert!(s.frames.len() <= 1001 && s.gpu.len() <= 1001);
    }

    #[test]
    fn gpu_times_are_summed_over_the_last_second() {
        let mut s = FrameStats::default();
        s.gpu(0.0, 50.0);
        s.gpu(2.0, 4.0);
        s.gpu(2.1, 8.0);
        s.gpu(2.2, f32::NAN);
        s.gpu(2.3, -1.0);
        s.frame(2.4, 1.0);
        let sum = s.summary(2.4);
        assert_eq!(sum.gpu_avg_ms, Some(6.0));
        assert_eq!(sum.gpu_worst_ms, Some(8.0));
        assert_eq!(FrameStats::default().summary(0.0), Summary::default());
    }

    #[test]
    fn hostile_numbers_stay_finite() {
        let mut s = FrameStats::default();
        s.frame(f64::NAN, f32::NAN);
        s.frame(1.0, f32::INFINITY);
        s.frame(0.5, -4.0);
        let sum = s.summary(1.0);
        assert!(sum.cpu_avg_ms.is_finite() && sum.fps.is_finite());
    }

    #[test]
    fn the_hud_lines_tell_the_story() {
        let sum = Summary {
            fps: 31.0,
            interval_avg_ms: 32.0,
            interval_worst_ms: 120.0,
            interval_worst_long_ms: 700.0,
            cpu_avg_ms: 2.5,
            cpu_worst_ms: 9.0,
            gpu_avg_ms: Some(28.0),
            gpu_worst_ms: Some(40.0),
            gpu_last_ms: None,
        };
        let ctx = Context {
            shapes: 12,
            instances: 3,
            groups: 2,
            scale: 0.5,
            adapter: "Vulkan / Test".into(),
            gpu_timing: true,
            fast: true,
            sharp: 0.0,
            pixels: [1264, 720],
            quality: "じどう".into(),
        };
        let l = lines(&sum, &ctx);
        let all = l.join("\n");
        assert!(all.contains("31 fps") && all.contains("最悪 120") && all.contains("700"));
        assert!(all.contains("GPU(シーン) 平均 28.0 / 最悪 40.0"));
        assert!(all.contains("倍率 0.50") && all.contains("1264x720") && all.contains("うごき中"));
        assert!(all.contains("形 12") && all.contains("コピー 3") && all.contains("Vulkan / Test"));
        // no timestamps: it says so; sharp in progress shows the share
        let none = Summary { gpu_avg_ms: None, gpu_worst_ms: None, ..sum };
        let c2 = Context { gpu_timing: false, fast: false, sharp: 0.4, ..ctx.clone() };
        let all = lines(&none, &c2).join("\n");
        assert!(all.contains("測れません") && all.contains("40%"));
        let c3 = Context { gpu_timing: true, fast: false, sharp: 1.0, ..ctx };
        assert!(lines(&none, &c3).join("\n").contains("計測まち"));
        let last = Summary { gpu_last_ms: Some(12.5), ..none };
        assert!(lines(&last, &c3).join("\n").contains("さいごの1回 12.5 ms"));
    }
}
