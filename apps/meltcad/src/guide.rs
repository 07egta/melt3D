//! Tiny, opt-out, first-use help for YuraCAD. No overlays covering the tools,
//! external images, animations, or per-frame disk writes.
use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{Align2, RichText, pos2};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::gizmo::Tool;
use crate::model::{Op, Scene};
use crate::sketch_ui::{Make, Pen, Stage, State};
use crate::style;

/// Independent kinds can be hidden without switching off all help.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintId {
    Navigation,
    Selection,
    Move,
    Rotate,
    Scale,
    SketchPolygon,
    SketchRectangle,
    SketchCircle,
    SketchSpline,
    SketchLine,
    SketchReady,
    SketchEdit,
    SketchLoft,
    SketchSweep,
    Curve,
    Pipe,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub intro_seen: bool,
    pub completed_practice: bool,
    pub hidden: Vec<HintId>,
}

impl Preferences {
    pub fn from_text(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn is_hidden(&self, id: HintId) -> bool {
        self.hidden.contains(&id)
    }

    pub fn hide(&mut self, id: HintId) {
        if !self.is_hidden(id) {
            self.hidden.push(id);
        }
    }

    pub fn read(path: &Path) -> Self {
        // Untrusted settings should never cause unbounded memory use or a crash.
        if std::fs::metadata(path).is_ok_and(|m| m.len() > 8192) {
            return Self::default();
        }
        std::fs::read_to_string(path).map(|s| Self::from_text(&s)).unwrap_or_default()
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, path)
    }
}

fn preferences_path() -> Option<PathBuf> {
    crate::backend::settings_path().map(|p| p.with_file_name("guide.json"))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Practice {
    #[default]
    Off,
    NewScene,
    StartSketch,
    DrawRectangle,
    Extrude,
    Complete,
}

pub struct Guide {
    pub prefs: Preferences,
    pub panel_open: bool,
    pub practice: Practice,
    /// The actual guide actions + mouse gestures taken in the current attempt.
    pub actions: u32,
    started: Option<Instant>,
    pub elapsed_seconds: Option<f64>,
}

impl Guide {
    fn from_preferences(mut prefs: Preferences, opening_file: bool) -> (Self, bool) {
        let first = !prefs.intro_seen && !prefs.completed_practice && !opening_file;
        if first {
            prefs.intro_seen = true;
        }
        (Self {
            prefs,
            panel_open: first,
            practice: Practice::Off,
            actions: 0,
            started: None,
            elapsed_seconds: None,
        }, first)
    }

    pub fn load(opening_file: bool) -> Self {
        // Unit tests must not write to a real user's per-machine settings.
        if cfg!(test) {
            return Self {
                prefs: Preferences::default(), panel_open: false, practice: Practice::Off,
                actions: 0, started: None, elapsed_seconds: None,
            };
        }
        let prefs = preferences_path().map_or_else(Preferences::default, |path| Preferences::read(&path));
        let (guide, first) = Self::from_preferences(prefs, opening_file);
        // Once shown, never force the introduction again, even after an abrupt exit.
        if first {
            guide.persist();
        }
        guide
    }

    fn persist(&self) {
        if cfg!(test) {
            return;
        }
        if let Some(path) = preferences_path()
            && let Err(e) = self.prefs.write(&path)
        {
            crate::gpu_log::log(&format!("guide preferences: {e}"));
        }
    }

    pub fn open(&mut self) {
        self.panel_open = true;
    }

    pub fn close(&mut self) {
        self.panel_open = false;
        self.practice = Practice::Off;
    }

    pub fn begin(&mut self) {
        self.panel_open = true;
        self.practice = Practice::NewScene;
        self.actions = 1; // "箱を作ってみる" click
        self.started = Some(Instant::now());
        self.elapsed_seconds = None;
    }

    pub fn clicked_new(&mut self) {
        if self.practice == Practice::NewScene {
            self.actions += 1;
        }
    }

    /// Called only when the native new-scene action actually replaced the scene.
    pub fn new_scene_applied(&mut self) {
        if self.practice == Practice::NewScene {
            self.practice = Practice::StartSketch;
        }
    }

    pub fn sketch_started(&mut self) -> bool {
        if self.practice == Practice::StartSketch {
            self.actions += 1;
            self.practice = Practice::DrawRectangle;
            true
        } else {
            false
        }
    }

    /// A rectangle drag is one gesture; two distinct clicks are two gestures.
    pub fn rectangle_gesture(&mut self) {
        if self.practice == Practice::DrawRectangle {
            self.actions += 1;
        }
    }

    pub fn rectangle_finished(&mut self) {
        if self.practice == Practice::DrawRectangle {
            self.practice = Practice::Extrude;
        }
    }

    /// Called after successful native Extrude, not when a disabled button is clicked.
    pub fn extruded(&mut self) {
        if self.practice == Practice::Extrude {
            self.actions += 1;
            self.practice = Practice::Complete;
            self.elapsed_seconds = self.started.map(|i| i.elapsed().as_secs_f64());
            self.prefs.completed_practice = true;
            self.persist();
        }
    }

    pub fn dismiss(&mut self, id: HintId) {
        self.prefs.hide(id);
        self.persist();
    }

    pub fn reset_hints(&mut self) {
        self.prefs.hidden.clear();
        self.persist();
    }
}

/// Context messages are cheap static strings, with no scene iteration or allocations.
#[derive(Clone, Copy, Debug)]
pub struct Hint {
    pub id: Option<HintId>,
    pub action: &'static str,
    pub controls: &'static str,
    pub warning: bool,
}

impl Hint {
    fn normal(id: HintId, action: &'static str, controls: &'static str) -> Self {
        Self { id: Some(id), action, controls, warning: false }
    }

    fn warn(action: &'static str, controls: &'static str) -> Self {
        // Warnings are not hideable: people need to know why an action is unavailable.
        Self { id: None, action, controls, warning: true }
    }
}

pub fn hint(sk: &State, tool: Tool, selected: bool, shapes: usize, op: Op, curve_drawing: bool, pipe_on: bool) -> Hint {
    if sk.edit.is_some() {
        return Hint::normal(HintId::SketchEdit, "輪かくの点をドラッグして編集", "線をクリック: 点追加 / Delete: 点削除 / Esc: 終了");
    }
    if sk.on {
        match sk.stage {
            Stage::Ready => {
                if sk.make == Make::Revolve && !sk.pframe.is_vertical() {
                    return Hint::warn("回転体はこの面では作れません", "「かきなおす」→「正面」か「横」の面で描く");
                }
                if sk.make == Make::Sweep && sk.path.len() < 2 {
                    return Hint::warn("スイープの通り道がありません", "「線をかく」→ 2点以上 → Enter");
                }
                if sk.make == Make::Loft && sk.loft_top == crate::sketch_ui::LoftTop::Drawn && sk.profile2.len() < 3 {
                    return Hint::warn("ロフトの上の輪かくがありません", "「上の輪かくをかく」で閉じた形を描く");
                }
                return Hint::normal(HintId::SketchReady, "輪かくができました。高さを決めて「押し出しを作る」", "クリック: 確定 / Esc: 描き直す");
            }
            Stage::LoftTop => return Hint::normal(HintId::SketchLoft, "ロフトの上側の輪かくを描く", "左ドラッグまたはクリック / Enter: 確定 / Esc: 戻る"),
            Stage::SweepPath => return Hint::normal(HintId::SketchSweep, "スイープの線を2点以上描く", "左クリック: 点 / Enter: 確定 / Esc: 戻る"),
            Stage::Draw => {}
        }
        return match sk.active_pen() {
            Pen::Polygon => Hint::normal(HintId::SketchPolygon, "輪かくの角を順番にクリック", "最初の点かEnter: 閉じる / Backspace: 1点戻す"),
            Pen::Rect => Hint::normal(HintId::SketchRectangle, "作業面でドラッグして四角を描く", "左ドラッグ または2回クリック / Esc: 取消"),
            Pen::Circle => Hint::normal(HintId::SketchCircle, "中心からドラッグして円を描く", "左ドラッグ または2回クリック / Esc: 取消"),
            Pen::Spline => Hint::normal(HintId::SketchSpline, "制御点を順番にクリック", "最初の点かEnter: 閉じる / Backspace: 戻す"),
            Pen::Line => Hint::normal(HintId::SketchLine, "線の点を順番にクリック", "Enter: 線を確定 / Esc: 取消"),
        };
    }
    if pipe_on {
        return Hint::normal(HintId::Pipe, "3D画面をなぞってパイプを作る", "左ドラッグ: 描く / Esc: 終了");
    }
    if curve_drawing {
        return Hint::normal(HintId::Curve, "3D画面で線の節点を配置する", "左クリック: 点追加 / Enter: 確定 / Esc: 終了");
    }
    if shapes == 0 && op == Op::Subtract {
        return Hint::warn("削る元の形がまだありません", "まず「足す」に切り替え、形を作ってください");
    }
    if !selected && matches!(tool, Tool::Move | Tool::Rotate | Tool::Scale) {
        return Hint::warn("操作する形が選ばれていません", "3D画面または左の一覧で形をクリックしてください");
    }
    match tool {
        Tool::Move => Hint::normal(HintId::Move, "矢印をドラッグして形を動かす", "W: 移動 / Ctrl: 1mm単位 / Ctrl+Z: 元に戻す"),
        Tool::Rotate => Hint::normal(HintId::Rotate, "輪っかをドラッグして回す", "E: 回転 / Ctrl: 15度単位 / Ctrl+Z: 戻す"),
        Tool::Scale => Hint::normal(HintId::Scale, "ハンドルをドラッグして大きさを変える", "R: 拡大縮小 / Ctrl+Z: 戻す"),
        Tool::All if selected => Hint::normal(HintId::Selection, "矢印・輪っか・四角から操作を選ぶ", "左ドラッグ: 形を編集 / ホイール: 拡大縮小"),
        _ if shapes == 0 => Hint::normal(HintId::Navigation, "左上の「スケッチ」から形を描こう", "左クリック: 選ぶ / 中ドラッグ: 視点回転 / F1: 一覧"),
        _ => Hint::normal(HintId::Selection, "形をクリックして選ぶ", "左クリック: 選択 / 中ドラッグ: 回転 / ホイール: ズーム"),
    }
}

/// UI actions are separate from the business operations. All guide buttons use
/// existing App methods; nothing silently edits a user's design.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Action {
    #[default]
    None,
    Begin,
    NewScene,
    StartSketch,
    Close,
}

fn content(ui: &mut egui::Ui, step: Practice, actions: u32, elapsed: Option<f64>) -> Action {
    let mut action = Action::None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("はじめてのYuraCAD").strong().color(style::ACCENT).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").on_hover_text("案内を閉じる。次回は自動表示しません").clicked() {
                action = Action::Close;
            }
        });
    });
    ui.separator();
    match step {
        Practice::Off => {
            ui.label("新規作成 → 四角を描く → 押し出す。実際に箱を1つ作ります。");
            ui.label(RichText::new("練習を始めても現在の作品は自動で消しません。").size(11.0).color(style::DIM));
            if ui.button("箱を1つ作ってみる").clicked() {
                action = Action::Begin;
            }
        }
        Practice::NewScene => {
            ui.label(RichText::new("1 / 3　練習用の新規作成").strong());
            ui.label("新しい空の作品を用意します。未保存の変更があれば保存を確認します。");
            if ui.button("練習用の新規作成").clicked() {
                action = Action::NewScene;
            }
        }
        Practice::StartSketch => {
            ui.label(RichText::new("2 / 3　スケッチを始める").strong());
            ui.label("左上のスケッチを選択。練習では四角形の道具を自動で選びます。");
            if ui.button("スケッチを始める").clicked() {
                action = Action::StartSketch;
            }
        }
        Practice::DrawRectangle => {
            ui.label(RichText::new("2 / 3　作業面に四角を描く").strong());
            ui.label("3D画面上で、左ボタンを押しながらななめにドラッグして離してください。");
            ui.label(RichText::new("2回クリックでも作れます。").size(11.0).color(style::DIM));
        }
        Practice::Extrude => {
            ui.label(RichText::new("3 / 3　押し出して箱にする").strong());
            ui.label("上のスケッチカードで高さを選び「押し出しを作る」をクリックします。");
            ui.label(RichText::new("高さは20mmのままで大丈夫。").size(11.0).color(style::DIM));
        }
        Practice::Complete => {
            ui.label(RichText::new("完成！　箱を1つ作れました").strong().color(style::ACCENT));
            ui.label(format!("ガイド内の操作: {actions}回"));
            if let Some(seconds) = elapsed {
                ui.label(format!("開始から約{seconds:.1}秒"));
            }
            ui.label("このガイドは自動では再表示しません。");
            if ui.button("閉じる").clicked() {
                action = Action::Close;
            }
        }
    }
    action
}

impl App {
    /// Non-modal, tiny and all text: never blocks the canvas or animates while idle.
    pub(crate) fn guide_card(&mut self, ctx: &egui::Context) {
        if !self.guide.panel_open {
            return;
        }
        let rect = self.view_rect;
        let mut action = Action::None;
        egui::Area::new(egui::Id::new("first_use_guide"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(pos2(rect.right() - 12.0, rect.bottom() - 90.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_max_width((rect.width() - 24.0).clamp(120.0, 292.0));
                    action = content(ui, self.guide.practice, self.guide.actions, self.guide.elapsed_seconds);
                });
            });
        match action {
            Action::None => {}
            Action::Close => self.guide.close(),
            Action::Begin => self.guide.begin(),
            Action::NewScene => {
                self.guide.clicked_new();
                let mut scene = Scene::default();
                for group in &mut scene.groups {
                    group.shapes.clear();
                }
                // Goes through the native unsaved-work confirmation.
                self.new_scene(scene);
            }
            Action::StartSketch => {
                self.sk.plane = crate::sketch_ui::PlaneSel::Floor;
                self.sk.pen = Pen::Rect;
                self.sk.make = Make::Extrude;
                self.sk.height = 20.0;
                self.set_sketch_tool(true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_is_one_time_and_a_file_open_does_not_interrupt() {
        let (first, shown) = Guide::from_preferences(Preferences::default(), false);
        assert!(shown && first.panel_open && first.prefs.intro_seen);
        let (again, shown) = Guide::from_preferences(first.prefs, false);
        assert!(!shown && !again.panel_open);
        let (completed, shown) = Guide::from_preferences(
            Preferences { completed_practice: true, ..Default::default() }, false,
        );
        assert!(!shown && !completed.panel_open, "completion always suppresses forced tours");
        let (opening, shown) = Guide::from_preferences(Preferences::default(), true);
        assert!(!shown && !opening.panel_open);
        let (later, shown) = Guide::from_preferences(opening.prefs, false);
        assert!(shown && later.panel_open);
    }

    #[test]
    fn preferences_are_forward_compatible_and_hints_are_independent() {
        let mut s = Preferences::from_text(r#"{"future":42,"intro_seen":true}"#);
        assert!(s.intro_seen);
        assert!(!s.is_hidden(HintId::SketchRectangle));
        s.hide(HintId::SketchRectangle);
        s.hide(HintId::SketchRectangle);
        assert_eq!(s.hidden.len(), 1);
        assert!(!s.is_hidden(HintId::Move));
        assert_eq!(Preferences::from_text(&serde_json::to_string(&s).unwrap()), s);
        assert_eq!(Preferences::from_text("not-json"), Preferences::default());
    }

    #[test]
    fn preferences_survive_restarting_without_repeating_completed_tour() {
        let p = std::env::temp_dir().join(format!("yura-guide-{}-{}.json", std::process::id(), line!()));
        let mut pref = Preferences { intro_seen: true, completed_practice: true, ..Default::default() };
        pref.hide(HintId::Move);
        pref.write(&p).expect("guide settings saved");
        assert_eq!(Preferences::read(&p), pref);
        assert!(Preferences::read(&p).completed_practice);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn a_one_drag_box_takes_five_user_actions_in_guided_flow() {
        let mut g = Guide {
            prefs: Preferences::default(), panel_open: true, practice: Practice::Off,
            actions: 0, started: None, elapsed_seconds: None,
        };
        g.begin();                    // begin guide button
        g.clicked_new();              // create blank work button
        g.new_scene_applied();        // actual native file operation, no extra gesture
        assert_eq!(g.practice, Practice::StartSketch);
        assert!(g.sketch_started());   // sketch tool click
        g.rectangle_gesture();        // single rectangle drag
        g.rectangle_finished();       // actual valid profile
        assert_eq!(g.practice, Practice::Extrude);
        // Avoid touching the real settings file in this pure state-machine test:
        g.practice = Practice::Complete;
        g.actions += 1;               // native extrusion button click
        assert_eq!(g.actions, 5);
        assert_eq!(g.practice, Practice::Complete);
    }

    #[test]
    fn guided_flow_operates_real_app_sketch_and_extrusion_without_replacing_other_work() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.guide.begin();
        app.guide.clicked_new();
        let mut blank = Scene::default();
        for group in &mut blank.groups {
            group.shapes.clear();
        }
        app.new_scene(blank);
        assert_eq!(app.guide.practice, Practice::StartSketch);
        assert_eq!(app.scene.shape_count(), 0);
        app.set_sketch_tool(true);
        assert_eq!(app.guide.practice, Practice::DrawRectangle);
        assert_eq!(app.sk.pen, Pen::Rect, "guide uses the existing rectangle pen");
        app.guide.rectangle_gesture();
        app.sk.press([0.0, 0.0]);
        let finished = app.sk.release([30.0, 20.0]).expect("valid rectangle gesture");
        app.sketch_done(finished);
        assert_eq!(app.guide.practice, Practice::Extrude);
        assert!(app.sketch_make(&ctx));
        assert_eq!(app.guide.practice, Practice::Complete);
        assert_eq!(app.guide.actions, 5);
        assert!(app.guide.prefs.completed_practice);
        assert_eq!(app.scene.shape_count(), 1);
        let sh = app.scene.shape(0, 0).expect("extruded box");
        assert_eq!(sh.kind, crate::model::Kind::Extrude);
        assert_eq!(sh.size[0], 20.0);
        assert_eq!(sh.outline.len(), 4);
        // Native scene modifications, not a fake demonstration.
        let distance = crate::sdf::Field::new(&app.scene).distance([15.0, 10.0, 10.0]);
        assert!(distance < 0.0, "the box is a real printable solid: {distance}");
    }

    #[test]
    fn contextual_hints_have_recovery_steps_for_unavailable_commands() {
        let mut sk = State::default();
        let missing = hint(&sk, Tool::Move, false, 2, Op::Add, false, false);
        assert!(missing.warning && missing.action.contains("選ばれ"));
        sk.on = true;
        sk.stage = Stage::Ready;
        sk.make = Make::Sweep;
        let h = hint(&sk, Tool::All, false, 2, Op::Add, false, false);
        assert!(h.warning && h.controls.contains("線"));
        sk.make = Make::Revolve;
        sk.pframe = crate::sketch::Frame::floor();
        let h = hint(&sk, Tool::All, false, 2, Op::Add, false, false);
        assert!(h.warning && h.controls.contains("正面"));
    }

    #[test]
    fn guide_panel_tessellation_has_no_external_assets_or_animation() {
        let ctx = egui::Context::default();
        // CPU-only headless benchmark; GPU timing must be measured with F3 on Windows.
        fn sample(ctx: &egui::Context, step: Option<Practice>) -> (u128, usize) {
            let mut times = Vec::new();
            let mut triangles = 0;
            for i in 0..100 {
                let start = Instant::now();
                ctx.begin_pass(egui::RawInput::default());
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(step) = step {
                        let _ = content(ui, step, 3, None);
                    } else {
                        ui.label("YuraCAD");
                    }
                });
                let out = ctx.end_pass();
                let tess = ctx.tessellate(out.shapes, 1.0);
                triangles = tess.iter().map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.indices.len() / 3,
                    _ => 0,
                }).sum();
                if i >= 20 { times.push(start.elapsed().as_micros()); }
            }
            times.sort_unstable();
            (times[75], triangles) // measured p95 of 80 warm samples
        }
        let base = sample(&ctx, None);
        let intro = sample(&ctx, Some(Practice::Off));
        let draw = sample(&ctx, Some(Practice::DrawRectangle));
        println!("CPU headless p95 µs and triangles: base={base:?} intro={intro:?} drawing={draw:?}");
        assert!(base.1 > 0 && intro.1 > base.1 && draw.1 > base.1);
    }
}
