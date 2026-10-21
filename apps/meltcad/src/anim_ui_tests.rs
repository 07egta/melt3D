//! Animation must wait for the viewport's cancelable work to finish.

use super::*;
use crate::curve_tool::NodeDrag;
use crate::model::Pick;
use crate::sketch::{self, Frame, Which};
use crate::sketch_ui::Edit;

fn animated() -> App {
    let mut app = App::new(true, None);
    let mut anim = std::mem::take(&mut app.scene.anim);
    anim.length = 12;
    anim.capture(&mut app.scene, 0).expect("first pose");
    app.scene.groups[0].shapes[0].pos[0] = 40.0;
    anim.capture(&mut app.scene, 8).expect("last pose");
    app.scene.anim = anim;
    app.an.open = true;
    app.show_pose(4);
    app.an.clock = 4.0;
    app
}

fn node_drag() -> NodeDrag {
    NodeDrag { g: 0, s: 0, i: 0, radius: false, handle: None, level: false, grab: [0.0; 3] }
}

fn bar(app: &mut App, ctx: &egui::Context) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1500.0, 300.0))),
        predicted_dt: 0.1,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.anim_bar(ui));
    output.textures_delta.clear();
}

#[test]
fn late_cancel_does_not_overwrite_preexisting_gif() {
    let path = std::env::temp_dir().join(format!("melt3d_cancel_{}_test.gif", std::process::id()));
    std::fs::write(&path, b"old contents").unwrap();
    let cancel = AtomicBool::new(true);
    assert!(save_gif_if_active(&path, b"new contents".to_vec(), &cancel).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"old contents");
    cancel.store(false, Ordering::Release);
    assert!(save_gif_if_active(&path, b"new contents".to_vec(), &cancel).is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), b"new contents");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn disconnected_gif_worker_releases_job_even_when_timeline_is_hidden() {
    let ctx = egui::Context::default();
    let mut app = animated();
    app.an.open = false;
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    app.an.job = Some(Job { rx, done: Arc::new(AtomicU32::new(2)), total: Arc::new(AtomicU32::new(12)), cancel: Arc::new(AtomicBool::new(false)) });
    bar(&mut app, &ctx);
    assert!(app.an.job.is_none(), "disconnected worker must not leave GIF progress stuck");
}

#[test]
fn hidden_timeline_keeps_polling_running_gif_and_releases_it_when_finished() {
    let ctx = egui::Context::default();
    let mut app = animated();
    app.an.open = false;
    let (tx, rx) = std::sync::mpsc::channel();
    app.an.job = Some(Job { rx, done: Arc::new(AtomicU32::new(5)), total: Arc::new(AtomicU32::new(12)), cancel: Arc::new(AtomicBool::new(false)) });
    bar(&mut app, &ctx);
    assert!(app.an.job.is_some());
    tx.send(Ok("test.gif".into())).unwrap();
    bar(&mut app, &ctx);
    assert!(app.an.job.is_none(), "hidden timeline must process completion");
}

#[test]
fn drafts_and_actual_drags_cannot_move_or_capture_a_pose() {
    for kind in ["sketch", "outline", "live", "source", "guide", "curve_drag", "curve_drawing", "pipe_trace", "gizmo_drag"] {
        let ctx = egui::Context::default();
        let mut app = animated();
        match kind {
            "sketch" => app.sk.on = true,
            "outline" => {
                app.sk.edit = Some(Edit { g: 0, s: 0, which: Which::Main, sel: None, drag: None, hover: None, edge_sel: None });
            }
            "live" => app.sk.live = Some((0, 0)),
            "source" => app.sk.source_sketch = Some((0, 0)),
            "guide" => app.sk.guide_edit = Some((0, 0)),
            "curve_drag" => app.ct.drag = Some(node_drag()),
            "curve_drawing" => app.ct.draw_shape = Some((0, 0)),
            "pipe_trace" => app.pt.tracing = true,
            "gizmo_drag" => {
                app.sel = Some(Pick::Shape(0, 0));
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0));
                let geom = crate::gizmo::build(&app.cam, rect, &app.scene, app.sel, app.tool).expect("gizmo");
                app.drag = Some(app.begin_drag(crate::gizmo::Handle::Centre, &geom, rect, rect.center(), app.tool, false));
            }
            _ => unreachable!(),
        }
        // This edit would differ from frame 4 and therefore create a key if
        // auto-capture merely waited for the mouse to be up.
        app.scene.groups[0].shapes[0].pos[0] += 3.0;
        let before = app.scene.clone();
        app.an.playing = true;
        bar(&mut app, &ctx);
        assert!(!app.an.playing, "{kind} pauses playback");
        assert_eq!(app.an.frame, 4, "{kind} keeps the playhead");
        app.show_pose(8);
        app.capture_animation_pose(&ctx);
        app.animation_capture_finished(&ctx);
        assert_eq!(app.scene, before, "{kind} cannot apply or record a partial edit");
        assert_eq!(app.an.frame, 4, "{kind} cannot scrub");
        assert_eq!(app.scene.anim.key_frames(), [0, 8]);
    }
}

#[test]
fn cancelling_a_live_solid_keeps_the_original_scene_and_poses() {
    let ctx = egui::Context::default();
    let mut app = animated();
    let before = app.scene.clone();
    app.sk.on = true;
    app.sk.separate = true;
    app.sk.pframe = Frame::horizontal(0.0);
    app.sk.profile = sketch::rect([0.0, 0.0], [20.0, 15.0]);
    app.sk.height = 12.0;
    assert!(app.live_begin());
    let draft = app.scene.clone();
    app.an.playing = true;
    bar(&mut app, &ctx);
    app.animation_capture_finished(&ctx);
    assert_eq!(app.scene, draft, "playback leaves the draft and its source in place");
    app.live_discard();
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    assert_eq!(app.scene, before, "cancel removes only the preview and creates no animation keys");
    app.undo();
    assert_eq!(app.scene, before, "the cancelled preview did not create an undo step");
}

#[test]
fn idle_curve_and_pipe_tools_still_allow_animation() {
    let ctx = egui::Context::default();
    let mut app = animated();
    app.ct.edit_nodes = true;
    app.ct.drawing = true;
    app.pt.on = true;
    assert!(!app.animation_edit_active());
    app.an.playing = true;
    bar(&mut app, &ctx);
    assert!(app.an.playing);
    assert_ne!(app.an.frame, 4, "ordinary playback advances");
    assert_eq!(app.scene.anim.key_frames(), [0, 8], "playback does not create keys");
    app.an.playing = false;
    app.show_pose(4);
    assert_eq!(app.scene.groups[0].shapes[0].pos[0], 20.0);
    app.scene.groups[0].shapes[0].pos[0] = 27.0;
    app.capture_animation_pose(&ctx);
    assert_eq!(app.scene.anim.key_frames(), [0, 4, 8]);
    app.show_pose(0);
    app.show_pose(4);
    assert_eq!(app.scene.groups[0].shapes[0].pos[0], 27.0, "the completed pose can be recorded and shown");
}

#[test]
fn a_released_edit_and_its_automatic_pose_are_one_undo_step() {
    let ctx = egui::Context::default();
    let mut app = animated();
    let before = app.scene.clone();
    app.ct.drag = Some(node_drag());
    app.scene.groups[0].shapes[0].pos[0] = 27.0;
    bar(&mut app, &ctx);
    assert_eq!(app.scene.anim.key_frames(), [0, 8]);
    // This is the frame_body order on release: viewport finishes the drag,
    // auto-capture runs, then settle stores both changes together.
    app.ct.drag = None;
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    let after = app.scene.clone();
    assert_eq!(after.anim.key_frames(), [0, 4, 8]);
    app.undo();
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    assert_eq!(app.scene, before, "one undo restores both geometry and animation");
    app.redo();
    assert_eq!(app.scene, after, "one redo restores the completed gesture and its pose");
}

#[test]
fn history_after_scrubbing_is_not_recaptured_at_the_new_frame() {
    let ctx = egui::Context::default();
    let mut app = animated();
    let before = app.scene.clone();
    app.scene.groups[0].shapes[0].pos[0] = 27.0;
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    app.show_pose(8);
    let shown = app.scene.clone();
    app.an.playing = true;
    app.undo();
    bar(&mut app, &ctx);
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    assert!(!app.an.playing, "undo pauses playback before it can overwrite the restored scene");
    assert_eq!(app.scene, before, "undo does not turn an old pose into a key at the current playhead");
    app.an.playing = true;
    app.redo();
    bar(&mut app, &ctx);
    app.animation_capture_finished(&ctx);
    app.settle(&ctx);
    assert!(!app.an.playing);
    assert_eq!(app.scene, shown, "redo preserves the exact saved history, including its original keys");
}

#[test]
fn disconnected_gif_worker_is_released_even_when_timeline_is_closed() {
    let ctx = egui::Context::default();
    let mut app = animated();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    app.an.job = Some(Job { rx, done: Arc::new(AtomicU32::new(0)), total: Arc::new(AtomicU32::new(1)), cancel: Arc::new(AtomicBool::new(false)) });
    app.an.open = false;
    bar(&mut app, &ctx);
    assert!(app.an.job.is_none(), "disconnected worker must not block future exports");
}

#[test]
fn closing_timeline_keeps_gif_job_pollable_and_cancellable() {
    let ctx = egui::Context::default();
    let mut app = animated();
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    app.an.job = Some(Job { rx, done: Arc::new(AtomicU32::new(1)), total: Arc::new(AtomicU32::new(3)), cancel: cancel.clone() });
    app.an.open = false;
    bar(&mut app, &ctx);
    assert!(app.an.job.is_some(), "pending job survives timeline close");
    cancel.store(true, Ordering::Relaxed);
    assert!(cancel.load(Ordering::Relaxed));
    tx.send(Err("Cancelled".to_string())).expect("deliver completion");
    bar(&mut app, &ctx);
    assert!(app.an.job.is_none(), "completion is polled with the timeline closed");
}
