//! Tests of live solids (`sketch_live.rs`): separate sketch confirmation, the internal direct
//! solid path, accept, discard, and hostile numbers.

use egui::{Rect, pos2, vec2};

use crate::app::App;
use crate::model::Kind;
use crate::sketch::{self, Frame};
use crate::sketch_live::{along_line, height_from};
use crate::sketch_ui::{Finished, Make, PlaneSel, Stage};

fn app() -> App {
    let mut app = App::new(true, None);
    app.view_rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 500.0));
    app
}

/// Use the retained internal path for tests of live solid mechanics. The public Sketch command
/// defaults to a saved guide; real input tests cover that two-command workflow.
fn direct_solid_tool(app: &mut App) {
    app.set_sketch_tool(true);
    app.sk.keep_sketch = false;
}

/// Complete the drawing gesture; its active command decides whether it is a draft or a solid.
fn closed_outline(app: &mut App, w: f32) {
    app.sketch_done(Finished::Profile(sketch::rect([0.0, 0.0], [w, 20.0])));
}

#[test]
fn height_and_line_maths_survive_hostile_numbers() {
    assert_eq!(height_from(f32::NAN, 1.0), None);
    assert_eq!(height_from(f32::INFINITY, 1.0), None);
    assert_eq!(height_from(12.4, 1.0), Some((12.0, false)));
    assert_eq!(height_from(-12.4, 1.0), Some((12.0, true)));
    assert_eq!(height_from(0.0, 1.0), Some((0.1, false)), "never flat");
    assert_eq!(height_from(1e30, 10.0), Some((2000.0, false)));
    assert_eq!(height_from(23.0, f32::NAN), Some((23.0, false)), "a bad step is 1 mm");
    assert_eq!(height_from(23.0, -4.0), Some((23.0, false)));
    assert_eq!(height_from(23.0, 10.0), Some((20.0, false)));
    assert!(along_line([0.0; 3], [0.0, 0.0, 1.0], [0.0; 3], [0.0, 0.0, 1.0]).is_none(), "a ray along the line");
    assert!(along_line([f32::NAN; 3], [1.0, 0.0, 0.0], [0.0; 3], [0.0, 1.0, 0.0]).is_none());
    let t = along_line([5.0, 0.0, 7.0], [0.0, 1.0, 0.0], [5.0, 3.0, 0.0], [0.0, 0.0, 1.0]);
    assert!(t.is_some_and(|t| (t - 7.0).abs() < 1e-4));
}

#[test]
fn direct_solid_closing_the_outline_puts_the_solid_in_the_scene() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    direct_solid_tool(&mut app);
    closed_outline(&mut app, 30.0);
    assert!(app.sk.live.is_some() && app.sk.stage == Stage::Shape, "the solid is live");
    assert_eq!(app.scene.shape_count(), n + 1);
    for _ in 0..3 {
        app.settle(&ctx);
    }
    assert!(app.sk.live.is_some(), "settling keeps the live solid");
    app.sk.height = 33.0;
    app.live_update();
    app.settle(&ctx);
    let Some((g, s)) = app.sk.live else { panic!("live") };
    assert_eq!(app.scene.shape(g, s).map(|sh| sh.size[0]), Some(33.0));
}

#[test]
fn direct_solid_accept_is_exactly_one_undo_step_and_discard_is_none() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n0 = app.scene.shape_count();
    direct_solid_tool(&mut app);
    // A: accepted
    closed_outline(&mut app, 30.0);
    app.sk.height = 8.0;
    app.live_update();
    app.settle(&ctx);
    app.live_accept(&ctx);
    app.settle(&ctx);
    assert!(app.sk.live.is_none());
    assert_eq!(app.scene.shape_count(), n0 + 1);
    // B: several height changes, then accepted
    closed_outline(&mut app, 12.0);
    for h in [5.0, 9.0, 14.0, 21.0] {
        app.sk.height = h;
        app.live_update();
        app.settle(&ctx);
    }
    app.live_accept(&ctx);
    app.settle(&ctx);
    assert_eq!(app.scene.shape_count(), n0 + 2);
    // C: discarded
    closed_outline(&mut app, 16.0);
    app.sk.height = 40.0;
    app.live_update();
    app.settle(&ctx);
    app.live_discard();
    app.settle(&ctx);
    assert!(app.sk.live.is_none());
    assert_eq!(app.scene.shape_count(), n0 + 2, "the discarded solid is gone");
    // one undo takes B away, the next takes A away: discard made no step, each accept made one
    app.undo();
    assert_eq!(app.scene.shape_count(), n0 + 1);
    app.undo();
    assert_eq!(app.scene.shape_count(), n0);
}

#[test]
fn direct_solid_hostile_heights_never_panic_and_never_leave_a_broken_solid() {
    let mut app = app();
    direct_solid_tool(&mut app);
    closed_outline(&mut app, 30.0);
    for h in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0, 1e-9, 1e30, 2000.0, 2001.0] {
        app.sk.height = h;
        app.live_update();
        let Some((g, s)) = app.sk.live else { panic!("the solid stays live") };
        let sh = app.scene.shape(g, s).expect("still there");
        assert!(sh.pos.iter().chain(sh.size.iter()).all(|v| v.is_finite()), "height {h}");
    }
    for s in [f32::NAN, -1.0, 0.0, 1e30] {
        app.sk.make = Make::Loft;
        app.sk.loft_scale = s;
        app.live_update();
    }
    app.live_discard();
    assert!(app.sk.live.is_none());
}

#[test]
fn a_sweep_path_makes_a_round_tube_that_discard_removes() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    app.set_sketch_tool(true);
    app.sketch_choose_make(Make::Sweep);
    assert_eq!(app.sk.stage, Stage::SweepPath);
    app.sk.frame = Frame::front();
    app.sketch_done(Finished::Line(vec![[0.0, 0.0], [0.0, 30.0], [30.0, 50.0]]));
    assert!(app.sk.live.is_some(), "the tube follows the path at once");
    assert_eq!(app.scene.shape_count(), n + 1);
    app.sk.sweep_scale = 1.5;
    app.live_update();
    app.settle(&ctx);
    app.live_discard();
    assert_eq!(app.scene.shape_count(), n);
    assert_eq!(app.sk.stage, Stage::SweepPath, "ready for another path");
}

#[test]
fn direct_solid_hole_icon_goes_back_to_the_ready_card_with_the_same_outline() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    direct_solid_tool(&mut app);
    closed_outline(&mut app, 30.0);
    app.live_to_ready();
    assert_eq!(app.sk.stage, Stage::Ready);
    assert_eq!(app.sk.profile.len(), 4, "the outline is kept");
    assert_eq!(app.scene.shape_count(), n, "the live solid was taken back");
    assert!(app.sketch_make(&ctx), "{}", app.sk.msg);
    assert_eq!(app.scene.shape_count(), n + 1);
}

#[test]
fn a_loft_gets_a_scaled_copy_on_top() {
    let mut app = app();
    app.set_sketch_tool(true);
    app.sketch_choose_make(Make::Loft);
    closed_outline(&mut app, 30.0);
    app.sk.loft_scale = 0.5;
    app.sk.height = 25.0;
    app.live_update();
    let Some((g, s)) = app.sk.live else { panic!("live") };
    let l = app.scene.shape(g, s).expect("there");
    assert_eq!(l.kind, Kind::Loft);
    assert_eq!(l.outline.len(), l.outline2.len());
}

#[test]
fn a_plain_sketch_is_confirmed_as_a_line_before_extrude_is_pressed() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    app.set_sketch_tool(true);
    app.sketch_set_plane(PlaneSel::Floor);
    assert!(app.sk.keep_sketch, "Sketch defaults to a separate drawing command");
    closed_outline(&mut app, 30.0);
    assert!(app.sk.live.is_none() && app.sk.stage == Stage::Ready);
    assert_eq!(app.scene.shape_count(), n, "dimensions can still change before the sketch is saved");
    assert!(app.sketch_confirm(&ctx));
    assert!(!app.sk.on && app.sk.live.is_none(), "Finish Sketch returns to selection");
    assert_eq!(app.scene.shape_count(), n + 1);
    let Some(crate::model::Pick::Shape(g, s)) = app.sel else { panic!("the line is selected") };
    assert_eq!(app.scene.shape(g, s).map(|sh| sh.kind), Some(Kind::Curve));
    assert!(app.selected_closed_curve().is_some());
    // Extrude on the selected line makes the solid, with the live handle, standing on the same plane
    assert!(app.make_from_selected_line(&ctx, Make::Extrude));
    assert!(app.sk.live.is_some() && app.sk.stage == Stage::Shape);
    assert_eq!(app.scene.shape_count(), n + 2);
    let Some((lg, ls)) = app.sk.live else { panic!("live") };
    let sh = app.scene.shape(lg, ls).expect("the live extrusion exists");
    assert_eq!(sh.kind, Kind::Extrude);
    assert_eq!(sh.outline, sketch::rect([-15.0, -10.0], [15.0, 10.0]), "the original 30 × 20 mm outline is preserved");
    assert_eq!(sh.size[0], app.sk.height, "the extrusion's first size parameter is its height");
    assert_eq!(sh.pos, [15.0, 10.0, app.sk.height * 0.5], "the extrusion stays over the original sketch");
}

#[test]
fn a_line_that_is_not_closed_is_not_extruded() {
    let ctx = egui::Context::default();
    let mut app = app();
    assert!(!app.make_from_selected_line(&ctx, Make::Extrude), "nothing selected");
}

#[test]
fn both_sides_centres_the_solid_on_the_plane() {
    let mut a = app();
    a.sk.make = Make::Extrude;
    closed_outline(&mut a, 30.0);
    let one = a.build_shape().map(|b| b.pos).unwrap_or([f32::NAN; 3]);
    a.sk.both = true;
    let two = a.build_shape().map(|b| b.pos).unwrap_or([f32::NAN; 3]);
    let moved = ((one[0] - two[0]).powi(2) + (one[1] - two[1]).powi(2) + (one[2] - two[2]).powi(2)).sqrt();
    assert!((moved - a.sk.height * 0.5).abs() < 1e-3, "moved {moved}");
}

#[test]
fn typed_dimension_updates_the_live_solid_before_enter() {
    use crate::sketch_dimensions::Dimensions;
    use crate::sketch_ui::Pen;
    let mut app = app();
    direct_solid_tool(&mut app);
    app.sk.pen = Pen::Rect;
    let raw = sketch::rect([0.0, 0.0], [20.0, 10.0]);
    app.sketch_done(Finished::Profile(raw.clone()));
    app.sk.dimension = Dimensions::for_shape(Pen::Rect, [0.0, 0.0], [20.0, 10.0], &raw, false);
    let Some((g, s)) = app.sk.live else { panic!("live solid expected") };
    let width = |app: &App| app.scene.shape(g, s).and_then(|sh| sketch::bbox(&sh.outline)).map(|(a, b)| b[0] - a[0]);
    assert_eq!(width(&app), Some(20.0));
    let Some(d) = app.sk.dimension.as_mut() else { panic!("editor missing") };
    d.fields = ["80".into(), "10".into()];
    app.dimension_field_changed(0);
    assert_eq!(width(&app), Some(80.0), "the solid follows the typed width without Enter (#212)");
}

#[test]
fn sweeping_a_bezier_curve_follows_the_drawn_arch() {
    use crate::model::{CurveHandles, CurveNodeMode, Material, Pick, Shape};
    let mut app = app();
    app.set_sketch_tool(true);
    let mut curve = Shape::new(Kind::Curve, [0.0; 3], Material::default());
    curve.path = vec![[-10.0, 0.0, 0.0, 1.0], [10.0, 0.0, 0.0, 1.0]];
    let up = [0.0, 20.0, 0.0];
    curve.curve_handles = vec![
        CurveHandles { mode: CurveNodeMode::Corner, incoming: [0.0; 3], outgoing: up },
        CurveHandles { mode: CurveNodeMode::Corner, incoming: up, outgoing: [0.0; 3] },
    ];
    app.scene.groups[0].shapes.push(curve);
    let idx = app.scene.groups[0].shapes.len() - 1;
    app.sel = Some(Pick::Shape(0, idx));
    app.sk.make = Make::Sweep;
    assert!(app.sweep_from_curve(), "{}", app.sk.msg);
    assert!(app.sk.path.len() > 2, "the sampled arch, not the two end nodes (#160)");
    let top = app.sk.path.iter().map(|p| p[1].max(p[2])).fold(f32::MIN, f32::max);
    assert!(top > 10.0, "the path rises with the arch: {top}");
}
