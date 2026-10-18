//! Tests of the one-gesture solid (`sketch_live.rs`): the live shape, accept, discard, hostile numbers.

use egui::{Rect, pos2, vec2};

use crate::app::App;
use crate::model::Kind;
use crate::sketch::{self, Frame};
use crate::sketch_live::{along_line, height_from};
use crate::sketch_ui::{Finished, Make, Stage};

fn app() -> App {
    let mut app = App::new(true, None);
    app.view_rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(800.0, 500.0));
    app
}

/// Close an outline: the solid appears at once.
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
fn closing_the_outline_puts_the_solid_in_the_scene() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    app.set_sketch_tool(true);
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
fn accept_is_exactly_one_undo_step_and_discard_is_none() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n0 = app.scene.shape_count();
    app.set_sketch_tool(true);
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
fn hostile_heights_never_panic_and_never_leave_a_broken_solid() {
    let mut app = app();
    app.set_sketch_tool(true);
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
fn the_hole_icon_goes_back_to_the_ready_card_with_the_same_outline() {
    let ctx = egui::Context::default();
    let mut app = app();
    let n = app.scene.shape_count();
    app.set_sketch_tool(true);
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
