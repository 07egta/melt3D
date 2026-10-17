//! Task-level usability recorder for YuraCAD/MELTCAD.
//! A human controls a real window. Headless egui tests are *not* human measurements.
//! In particular, an egui frame completing is NOT a photon reaching the screen.

use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use egui::{Context, Event, Pos2, Rect};
use serde::Serialize;

const MAX_SAMPLES: usize = 30_000;
const IDLE_CUTOFF_SECS: f64 = 0.25;
const WHEEL_GESTURE_SECS: f64 = 0.20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Rectangle100x50,
    CircleExtrude,
    SplineNodeEdit,
    Transform,
    SketchReedit,
    SaveReload,
}

impl Scenario {
    pub const ALL: [Self; 6] = [
        Self::Rectangle100x50, Self::CircleExtrude, Self::SplineNodeEdit,
        Self::Transform, Self::SketchReedit, Self::SaveReload,
    ];
    pub const fn id(self) -> &'static str {
        match self {
            Self::Rectangle100x50 => "rectangle_100x50",
            Self::CircleExtrude => "circle_extrude",
            Self::SplineNodeEdit => "spline_node_edit",
            Self::Transform => "transform_move_rotate_scale",
            Self::SketchReedit => "sketch_reedit",
            Self::SaveReload => "save_reload",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rectangle100x50 => "100×50 mm の四角を作る",
            Self::CircleExtrude => "円を描いて押し出す",
            Self::SplineNodeEdit => "スプラインの点を移動",
            Self::Transform => "移動・回転・拡大",
            Self::SketchReedit => "押し出したスケッチを再編集",
            Self::SaveReload => "保存・再読み込み",
        }
    }
    pub const fn success(self) -> &'static str {
        match self {
            Self::Rectangle100x50 => "スケッチの幅100mm、高さ50mmを確認",
            Self::CircleExtrude => "円の閉じた輪郭を指定高さで押し出した",
            Self::SplineNodeEdit => "制御点が動き、曲線が更新された",
            Self::Transform => "同じ形状の移動・回転・拡大をすべて反映",
            Self::SketchReedit => "既存形状の輪郭を変え、3Dを更新",
            Self::SaveReload => "保存したファイルを開き直して形状・配置を確認",
        }
    }
}
impl Default for Scenario {
    fn default() -> Self { Self::Rectangle100x50 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode { Human, Automated }

#[derive(Clone, Debug, Serialize)]
pub struct Conditions {
    pub build_id: String,
    pub machine: String,
    pub os: String,
    pub physical_window_px: [u32; 2],
    pub pixels_per_point: f32,
    pub quality: String,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    /// One click or drag = one pointer press; one non-repeated keypress = one action.
    /// Wheel bursts less than 200 ms apart = one action. Pointer motion alone = zero.
    pub actions: u32,
    /// Marked explicitly by the tester, NEVER inferred from a selection change.
    pub unwanted_selections: u32,
    /// Marked explicitly by the tester, NEVER inferred from Escape/Undo.
    pub unwanted_cancels: u32,
    /// Human: real monotonic wall-clock time; automated: elapsed runner time, not human time.
    pub completion_ms: f64,
    /// Active UI frame scheduling samples, excluding idle gaps >=250 ms (NOT GPU present FPS).
    pub ui_frame_fps: Option<f64>,
    pub ui_frame_interval_p95_ms: Option<f64>,
    pub ui_cpu_frame_p95_ms: Option<f64>,
    pub active_frame_samples: usize,
    /// Not measured by egui: requires an external camera / OS presentation timestamps.
    pub input_to_screen_latency_ms: Option<f64>,
    pub display_fps: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub scenario: Scenario,
    pub mode: Mode,
    pub passed: bool,
    pub stage: String,
    pub conditions: Conditions,
    pub metrics: Metrics,
    pub measured_at_unix_ms: u128,
    pub limitations: Vec<String>,
}

/// Reusable measurement engine. The automated tests may feed synthetic events, but their
/// 16 ms simulated frames are not counted as real-world FPS.
pub struct Trial {
    pub scenario: Scenario,
    pub mode: Mode,
    pub stage: String,
    pub conditions: Conditions,
    started: Instant,
    actions: u32,
    unwanted_selections: u32,
    unwanted_cancels: u32,
    ui_intervals_ms: Vec<f64>,
    cpu_frames_ms: Vec<f64>,
    last_active_at: Option<f64>,
    last_wheel_at: Option<f64>,
    first_frame: bool,
}

impl Trial {
    pub fn new(scenario: Scenario, mode: Mode, stage: String, conditions: Conditions) -> Self {
        Self {
            scenario, mode, stage, conditions, started: Instant::now(), actions: 0,
            unwanted_selections: 0, unwanted_cancels: 0,
            ui_intervals_ms: Vec::new(), cpu_frames_ms: Vec::new(),
            last_active_at: None, last_wheel_at: None, first_frame: true,
        }
    }

    /// Use only app-origin events; exclude clicks inside the recorder window.
    /// The recorder's Start press (first frame) is intentionally not an action.
    pub fn observe(
        &mut self, events: &[Event], now: f64, ui_cpu_ms: f64,
        pointer_down: bool, wheel_delta: f32, recorder_rect: Option<Rect>,
    ) {
        if self.first_frame {
            self.first_frame = false;
            return;
        }
        let own = |p: Pos2| recorder_rect.is_some_and(|r| r.contains(p));
        let mut active = false;
        for e in events {
            match e {
                Event::PointerButton { pos, pressed: true, .. } if !own(*pos) => {
                    self.actions = self.actions.saturating_add(1);
                    active = true;
                }
                Event::PointerMoved(p) if !own(*p) => active = true,
                Event::Key { pressed: true, repeat: false, .. } => {
                    self.actions = self.actions.saturating_add(1);
                    active = true;
                }
                Event::Paste(_) => {
                    self.actions = self.actions.saturating_add(1);
                    active = true;
                }
                _ => {}
            }
        }
        if wheel_delta.abs() > 0.0 {
            active = true;
            if self.last_wheel_at.is_none_or(|t| now - t > WHEEL_GESTURE_SECS || now < t) {
                self.actions = self.actions.saturating_add(1);
            }
            self.last_wheel_at = Some(now);
        }
        if pointer_down {
            active = true;
        }
        if !active || !now.is_finite() {
            self.last_active_at = None;
            return;
        }
        if let Some(t) = self.last_active_at {
            let delta = now - t;
            if delta > 0.0 && delta < IDLE_CUTOFF_SECS && self.ui_intervals_ms.len() < MAX_SAMPLES {
                self.ui_intervals_ms.push(delta * 1000.0);
            }
        }
        self.last_active_at = Some(now);
        if ui_cpu_ms.is_finite() && ui_cpu_ms >= 0.0 && self.cpu_frames_ms.len() < MAX_SAMPLES {
            self.cpu_frames_ms.push(ui_cpu_ms);
        }
    }
    pub fn mark_unwanted_selection(&mut self) {
        self.unwanted_selections = self.unwanted_selections.saturating_add(1);
    }
    pub fn mark_unwanted_cancel(&mut self) {
        self.unwanted_cancels = self.unwanted_cancels.saturating_add(1);
    }
    pub fn actions(&self) -> u32 { self.actions }
    pub fn errors(&self) -> (u32, u32) { (self.unwanted_selections, self.unwanted_cancels) }

    pub fn finish(self, passed: bool) -> Report {
        let ui_p95 = if self.mode == Mode::Human { p95(&self.ui_intervals_ms) } else { None };
        let cpu_p95 = p95(&self.cpu_frames_ms);
        let fps = if self.mode == Mode::Human && self.ui_intervals_ms.len() >= 5 {
            let avg = self.ui_intervals_ms.iter().sum::<f64>() / self.ui_intervals_ms.len() as f64;
            if avg > 0.0 { Some(1000.0 / avg) } else { None }
        } else {
            None
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        Report {
            schema_version: 1, scenario: self.scenario, mode: self.mode, passed,
            stage: self.stage, conditions: self.conditions,
            metrics: Metrics {
                actions: self.actions,
                unwanted_selections: self.unwanted_selections,
                unwanted_cancels: self.unwanted_cancels,
                completion_ms: self.started.elapsed().as_secs_f64() * 1000.0,
                ui_frame_fps: fps,
                ui_frame_interval_p95_ms: ui_p95,
                ui_cpu_frame_p95_ms: cpu_p95,
                active_frame_samples: self.cpu_frames_ms.len(),
                input_to_screen_latency_ms: None,
                display_fps: None,
            },
            measured_at_unix_ms: now,
            limitations: vec![
                "input_to_screen_latency_ms and display_fps are unmeasured: UI frames do not prove display presentation".into(),
                "UI FPS is based on active egui frame intervals; it is not measured GPU presentation FPS".into(),
                "Unwanted selections/cancels require an explicit tester mark; Escape and Undo are not automatically errors".into(),
                "Automated test elapsed time and human task time must never be pooled".into(),
            ],
        }
    }
}

fn p95(samples: &[f64]) -> Option<f64> {
    if samples.len() < 5 { return None; }
    let mut values: Vec<f64> = samples.iter().copied().filter(|x| x.is_finite() && *x >= 0.0).collect();
    if values.len() < 5 { return None; }
    values.sort_by(f64::total_cmp);
    let i = ((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    values.get(i).copied()
}

/// Create-only save: never overwrites an earlier trial, even if the machine clock repeats.
pub fn save_report(report: &Report) -> Result<std::path::PathBuf, String> {
    let json = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join("yuracad-ux");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for n in 0..1000 {
        let path = dir.join(format!("{}-{}-{n}.json", report.scenario.id(), report.measured_at_unix_ms));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(&json).map_err(|e| format!("{}: {e}", path.display()))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Unable to allocate a unique UX report filename".into())
}

/// Small overlay for real human trials; it is deliberately not exposed for automatic tests.
pub struct Ui {
    pub open: bool,
    selected: Scenario,
    stage: String,
    build_id: String,
    machine: String,
    notes: String,
    active: Option<Trial>,
    pub last_report: Option<Report>,
    pub saved: String,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            open: false, selected: Scenario::default(), stage: "before".into(),
            build_id: String::new(), machine: String::new(), notes: String::new(),
            active: None, last_report: None, saved: String::new(),
        }
    }
}

impl Ui {
    pub fn show(&mut self, ctx: &Context, quality: &str) {
        if !self.open { return; }
        let mut opened = self.open;
        let mut start = false;
        let mut stop: Option<bool> = None;
        egui::Window::new("操作性テスト（人間による測定）")
            .id(egui::Id::new("ux_workflow_bench"))
            .open(&mut opened)
            .default_width(340.0)
            .show(ctx, |ui| {
                if let Some(run) = self.active.as_mut() {
                    ui.label(format!("計測中：{}", run.scenario.label()));
                    ui.label(format!("操作回数：{}", run.actions()));
                    let (sel, cancel) = run.errors();
                    ui.label(format!("誤選択：{sel}　意図しない取消：{cancel}"));
                    if ui.button("誤選択を1件記録").clicked() { run.mark_unwanted_selection(); }
                    if ui.button("意図しない取消を1件記録").clicked() { run.mark_unwanted_cancel(); }
                    ui.horizontal(|ui| {
                        if ui.button("成功として完了").clicked() { stop = Some(true); }
                        if ui.button("失敗・中断").clicked() { stop = Some(false); }
                    });
                    ui.small("計測パネル内のクリックは操作回数から除外します。");
                } else {
                    egui::ComboBox::from_label("シナリオ")
                        .selected_text(self.selected.label())
                        .show_ui(ui, |ui| {
                            for scenario in Scenario::ALL {
                                ui.selectable_value(&mut self.selected, scenario, scenario.label());
                            }
                        });
                    ui.small(self.selected.success());
                    ui.horizontal(|ui| {
                        ui.label("条件");
                        ui.selectable_value(&mut self.stage, "before".into(), "改善前");
                        ui.selectable_value(&mut self.stage, "after".into(), "改善後");
                    });
                    ui.label("ビルドID / commit SHA（必須）");
                    ui.text_edit_singleline(&mut self.build_id);
                    ui.label("PC / GPU / ドライバー（必須）");
                    ui.text_edit_singleline(&mut self.machine);
                    ui.label("データ・条件のメモ");
                    ui.text_edit_singleline(&mut self.notes);
                    if ui.add_enabled(!self.build_id.trim().is_empty() && !self.machine.trim().is_empty(), egui::Button::new("計測開始")).clicked() {
                        start = true;
                    }
                    if self.last_report.is_some() && ui.button("直近の記録をコピー（JSON）").clicked() {
                        if let Some(r) = &self.last_report {
                            if let Ok(s) = serde_json::to_string_pretty(r) { ctx.copy_text(s); }
                        }
                    }
                    if !self.saved.is_empty() { ui.small(&self.saved); }
                }
                ui.separator();
                ui.small("入力→実画面の遅延と実ディスプレイFPSは未測定。UI更新間隔・CPU時間と混同しません。");
            });
        self.open = opened;
        if start {
            let rect = ctx.screen_rect();
            let ppp = ctx.pixels_per_point();
            let condition = Conditions {
                build_id: self.build_id.clone(), machine: self.machine.clone(),
                os: std::env::consts::OS.to_string(),
                physical_window_px: [(rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32],
                pixels_per_point: ppp, quality: quality.into(), notes: self.notes.clone(),
            };
            self.active = Some(Trial::new(self.selected, Mode::Human, self.stage.clone(), condition));
            self.saved.clear();
        }
        if let Some(passed) = stop {
            if let Some(trial) = self.active.take() {
                let report = trial.finish(passed);
                self.saved = match save_report(&report) {
                    Ok(path) => format!("保存：{}", path.display()),
                    Err(e) => format!("保存失敗：{e}（JSONコピーを利用してください）"),
                };
                self.last_report = Some(report);
            }
        }
        if self.active.is_some() { ctx.request_repaint_after(std::time::Duration::from_millis(120)); }
    }

    /// Called at the END of frame_body. Includes all app input processing before the UI frame ends.
    pub fn record_frame(&mut self, ctx: &Context, ui_cpu_ms: f64) {
        let Some(t) = self.active.as_mut() else { return };
        let r = ctx.memory(|m| m.area_rect(egui::Id::new("ux_workflow_bench")));
        let (now, events, pointer_down, wheel, pointer_pos) = ctx.input(|i| (
            i.time, i.events.clone(), i.pointer.any_down(), i.smooth_scroll_delta.y,
            i.pointer.interact_pos(),
        ));
        let is_on_recorder = pointer_pos.is_some_and(|p| r.is_some_and(|rect| rect.contains(p)));
        let events = if is_on_recorder {
            // Ignore keyboard shortcuts, clicks and drags in the recording overlay.
            Vec::new()
        } else { events };
        t.observe(&events, now, ui_cpu_ms, pointer_down && !is_on_recorder,
            if is_on_recorder { 0.0 } else { wheel }, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn conditions() -> Conditions {
        Conditions {
            build_id: "test-sha".into(), machine: "simulator".into(), os: "test".into(),
            physical_window_px: [1280, 720], pixels_per_point: 1.0,
            quality: "normal".into(), notes: String::new(),
        }
    }
    fn down() -> Event {
        Event::PointerButton {
            pos: egui::pos2(200.0, 200.0), button: egui::PointerButton::Primary,
            pressed: true, modifiers: Default::default(),
        }
    }
    #[test]
    fn all_six_scenarios_have_distinct_stable_ids() {
        let set: std::collections::HashSet<_> = Scenario::ALL.iter().map(|s| s.id()).collect();
        assert_eq!(set.len(), 6);
    }
    #[test]
    fn a_drag_is_one_action_and_escape_is_not_automatically_an_error() {
        let mut t = Trial::new(Scenario::Rectangle100x50, Mode::Automated, "before".into(), conditions());
        t.observe(&[down()], 0.0, 1.0, true, 0.0, None); // recorder Start frame excluded
        t.observe(&[down()], 0.016, 2.0, true, 0.0, None);
        for k in 2..12 {
            t.observe(&[Event::PointerMoved(egui::pos2(200.0 + k as f32, 200.0))],
                k as f64 * 0.016, 2.0, true, 0.0, None);
        }
        t.observe(&[], 0.192, 2.0, false, 0.0, None);
        assert_eq!(t.actions(), 1);
        assert_eq!(t.errors(), (0, 0));
        t.mark_unwanted_selection();
        t.mark_unwanted_cancel();
        let r = t.finish(true);
        assert_eq!((r.metrics.unwanted_selections, r.metrics.unwanted_cancels), (1, 1));
        assert!(r.metrics.ui_frame_fps.is_none(), "synthetic time is not real FPS");
        assert!(r.metrics.input_to_screen_latency_ms.is_none());
        assert!(r.metrics.ui_cpu_frame_p95_ms.is_some());
    }
    #[test]
    fn inactive_gaps_do_not_become_slow_frames() {
        let mut t = Trial::new(Scenario::SaveReload, Mode::Human, "after".into(), conditions());
        t.observe(&[], 0.0, 1.0, false, 0.0, None);
        for i in 1..8 { t.observe(&[down()], i as f64 * 0.016, 2.0, true, 0.0, None); }
        t.observe(&[], 3.0, 2.0, false, 0.0, None);
        for i in 0..8 { t.observe(&[down()], 3.016 + i as f64 * 0.016, 2.0, true, 0.0, None); }
        let report = t.finish(true);
        assert!(report.metrics.ui_frame_interval_p95_ms.unwrap_or(f64::INFINITY) < 20.0);
        assert!(report.metrics.ui_frame_fps.is_some());
    }
    #[test]
    fn nearest_rank_percentile_and_overlay_exclusion() {
        assert_eq!(p95(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(5.0));
        let mut t = Trial::new(Scenario::CircleExtrude, Mode::Human, "before".into(), conditions());
        let panel = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 100.0));
        t.observe(&[], 0.0, 1.0, false, 0.0, Some(panel));
        t.observe(&[Event::PointerButton {
            pos: egui::pos2(20.0, 20.0), button: egui::PointerButton::Primary,
            pressed: true, modifiers: Default::default(),
        }], 0.016, 1.0, false, 0.0, Some(panel));
        assert_eq!(t.actions(), 0);
    }
}
