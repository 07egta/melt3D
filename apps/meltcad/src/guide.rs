//! Tiny, opt-out, first-use help for YuraCAD. No overlays covering the tools,
//! external images, animations, or per-frame disk writes.
use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{Align2, RichText, pos2};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::gizmo::Tool;
use crate::i18n::tr;
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
    SketchHole,
    SketchPick,
    SketchLive,
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
        (Self { prefs, panel_open: first, practice: Practice::Off, actions: 0, started: None, elapsed_seconds: None }, first)
    }

    pub fn load(opening_file: bool) -> Self {
        // Unit tests must not write to a real user's per-machine settings.
        if cfg!(test) {
            return Self {
                prefs: Preferences::default(),
                panel_open: false,
                practice: Practice::Off,
                actions: 0,
                started: None,
                elapsed_seconds: None,
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
        Self { id: Some(id), action: tr(action), controls: tr(controls), warning: false }
    }

    fn warn(action: &'static str, controls: &'static str) -> Self {
        // Warnings are not hideable: people need to know why an action is unavailable.
        Self { id: None, action: tr(action), controls: tr(controls), warning: true }
    }
}

/// The next step for the tool in use. `action` and `controls` are catalog keys here; the
/// returned [`Hint`] carries the translated text.
pub fn hint(sk: &State, tool: Tool, selected: bool, shapes: usize, op: Op, curve_drawing: bool, pipe_on: bool) -> Hint {
    if sk.edit.is_some() {
        return Hint::normal(HintId::SketchEdit, "guide.hint.edit", "guide.hint.edit.keys");
    }
    if sk.on {
        match sk.stage {
            Stage::Ready => {
                if sk.make == Make::Revolve && !sk.pframe.is_vertical() {
                    return Hint::warn("guide.warn.revolve_plane", "guide.warn.revolve_plane.fix");
                }
                if sk.make == Make::Sweep && sk.path.len() < 2 {
                    return Hint::warn("guide.warn.sweep_path", "guide.warn.sweep_path.fix");
                }
                if sk.make == Make::Loft && sk.loft_top == crate::sketch_ui::LoftTop::Drawn && sk.profile2.len() < 3 {
                    return Hint::warn("guide.warn.loft_top", "guide.warn.loft_top.fix");
                }
                if sk.keep_sketch {
                    return Hint::normal(HintId::SketchReady, "guide.hint.ready.keep", "guide.hint.ready.keep.keys");
                }
                let action = match sk.make {
                    Make::Extrude => "guide.hint.ready.extrude",
                    Make::Revolve => "guide.hint.ready.revolve",
                    Make::Loft => "guide.hint.ready.loft",
                    Make::Sweep => "guide.hint.ready.sweep",
                };
                return Hint::normal(HintId::SketchReady, action, "guide.hint.ready.keys");
            }
            Stage::Shape => return Hint::normal(HintId::SketchLive, "guide.hint.live", "guide.hint.live.keys"),
            Stage::LoftTop => return Hint::normal(HintId::SketchLoft, "guide.hint.loft", "guide.hint.loft.keys"),
            Stage::SweepPath => return Hint::normal(HintId::SketchSweep, "guide.hint.sweep", "guide.hint.sweep.keys"),
            Stage::Hole => return Hint::normal(HintId::SketchHole, "guide.hint.hole", "guide.hint.hole.keys"),
            Stage::Draw => {}
        }
        if sk.picking {
            return Hint::normal(HintId::SketchPick, "guide.hint.pick", "guide.hint.pick.keys");
        }
        return match sk.active_pen() {
            Pen::Polygon => Hint::normal(HintId::SketchPolygon, "guide.hint.polygon", "guide.hint.points.keys"),
            Pen::Rect => Hint::normal(HintId::SketchRectangle, "guide.hint.rect", "guide.hint.drag.keys"),
            Pen::Circle => Hint::normal(HintId::SketchCircle, "guide.hint.circle", "guide.hint.drag.keys"),
            Pen::Spline => Hint::normal(HintId::SketchSpline, "guide.hint.spline", "guide.hint.points.keys"),
            Pen::Line => Hint::normal(HintId::SketchLine, "guide.hint.line", "guide.hint.line.keys"),
        };
    }
    if pipe_on {
        return Hint::normal(HintId::Pipe, "guide.hint.pipe", "guide.hint.pipe.keys");
    }
    if curve_drawing {
        return Hint::normal(HintId::Curve, "guide.hint.curve", "guide.hint.curve.keys");
    }
    if shapes == 0 && op == Op::Subtract {
        return Hint::warn("guide.warn.nothing_to_cut", "guide.warn.nothing_to_cut.fix");
    }
    if !selected && matches!(tool, Tool::Move | Tool::Rotate | Tool::Scale) {
        return Hint::warn("guide.warn.no_selection", "guide.warn.no_selection.fix");
    }
    match tool {
        Tool::Move => Hint::normal(HintId::Move, "guide.hint.move", "guide.hint.move.keys"),
        Tool::Rotate => Hint::normal(HintId::Rotate, "guide.hint.rotate", "guide.hint.rotate.keys"),
        Tool::Scale => Hint::normal(HintId::Scale, "guide.hint.scale", "guide.hint.scale.keys"),
        Tool::All if selected => Hint::normal(HintId::Selection, "guide.hint.all", "guide.hint.all.keys"),
        _ if shapes == 0 => Hint::normal(HintId::Navigation, "guide.hint.empty", "guide.hint.empty.keys"),
        _ => Hint::normal(HintId::Selection, "guide.hint.select", "guide.hint.select.keys"),
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
    let note = |ui: &mut egui::Ui, key: &str| {
        ui.label(RichText::new(tr(key)).size(11.0).color(style::DIM));
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("guide.title")).strong().color(style::ACCENT).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").on_hover_text(tr("guide.close.tip")).clicked() {
                action = Action::Close;
            }
        });
    });
    ui.separator();
    match step {
        Practice::Off => {
            ui.label(tr("guide.intro"));
            note(ui, "guide.intro.safe");
            if ui.button(tr("guide.begin")).clicked() {
                action = Action::Begin;
            }
        }
        Practice::NewScene => {
            ui.label(RichText::new(format!("1 / 3　{}", tr("guide.new.title"))).strong());
            ui.label(tr("guide.new.body"));
            if ui.button(tr("guide.new.title")).clicked() {
                action = Action::NewScene;
            }
        }
        Practice::StartSketch => {
            ui.label(RichText::new(format!("2 / 3　{}", tr("guide.sketch.title"))).strong());
            ui.label(tr("guide.sketch.body"));
            if ui.button(tr("guide.sketch.title")).clicked() {
                action = Action::StartSketch;
            }
        }
        Practice::DrawRectangle => {
            ui.label(RichText::new(format!("2 / 3　{}", tr("guide.rect.title"))).strong());
            ui.label(tr("guide.rect.body"));
            note(ui, "guide.rect.note");
        }
        Practice::Extrude => {
            ui.label(RichText::new(format!("3 / 3　{}", tr("guide.extrude.title"))).strong());
            ui.label(tr("guide.extrude.body"));
            note(ui, "guide.extrude.note");
        }
        Practice::Complete => {
            ui.label(RichText::new(tr("guide.done.title")).strong().color(style::ACCENT));
            ui.label(format!("{}: {actions}", tr("guide.done.actions")));
            if let Some(seconds) = elapsed {
                ui.label(format!("{}: {seconds:.1}", tr("guide.done.seconds")));
            }
            ui.label(tr("guide.done.note"));
            if ui.button(tr("guide.done.close")).clicked() {
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
            .fixed_pos(pos2(rect.right() - 16.0, rect.bottom() - 16.0))
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
            // `set_sketch_tool` sees the practice step and picks the floor, the rectangle and
            // a live extrusion itself (the same as the Sketch button during the practice).
            Action::StartSketch => self.set_sketch_tool(true),
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
        let (completed, shown) = Guide::from_preferences(Preferences { completed_practice: true, ..Default::default() }, false);
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
        let mut g =
            Guide { prefs: Preferences::default(), panel_open: true, practice: Practice::Off, actions: 0, started: None, elapsed_seconds: None };
        g.begin(); // begin guide button
        g.clicked_new(); // create blank work button
        g.new_scene_applied(); // actual native file operation, no extra gesture
        assert_eq!(g.practice, Practice::StartSketch);
        assert!(g.sketch_started()); // sketch tool click
        g.rectangle_gesture(); // single rectangle drag
        g.rectangle_finished(); // actual valid profile
        assert_eq!(g.practice, Practice::Extrude);
        // Avoid touching the real settings file in this pure state-machine test:
        g.practice = Practice::Complete;
        g.actions += 1; // native extrusion button click
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
                let input =
                    egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 540.0))), ..Default::default() };
                let out = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        if let Some(step) = step {
                            let _ = content(ui, step, 3, None);
                        } else {
                            ui.label("YuraCAD");
                        }
                    });
                });
                let tess = ctx.tessellate(out.shapes, out.pixels_per_point);
                triangles = tess
                    .iter()
                    .map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(mesh) => mesh.indices.len() / 3,
                        _ => 0,
                    })
                    .sum();
                if i >= 20 {
                    times.push(start.elapsed().as_micros());
                }
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
