//! The animation bar: a timeline under the view. Pose the scene, press "Set pose", go to another
//! frame, change things, and the poses are kept (editing a frame that already has poses sets its
//! pose by itself). Play shows the frames in between; "Clay" holds each pose like stop motion.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{RichText, Sense, Ui, pos2, vec2};

use crate::app::App;
use crate::style;

pub struct Job {
    rx: Receiver<Result<String, String>>,
    done: Arc<AtomicU32>,
    total: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
}

/// A cancelled GIF must not overwrite a previous export when its final
/// image has already rendered. Check cancellation again at the I/O boundary.
fn save_gif_if_active(path: &std::path::Path, bytes: Vec<u8>, cancel: &AtomicBool) -> Result<String, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Stopped".into());
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub frame: u32,
    pub playing: bool,
    clock: f32,
    /// The scene revision last looked at for edits.
    rev: u64,
    job: Option<Job>,
}

impl App {
    /// Poll GIF completion even when the timeline itself is closed.
    fn poll_gif_job(&mut self, ctx: &egui::Context) {
        let finished = self.an.job.as_ref().and_then(|job| {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            match job.rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("GIF worker ended without a result".to_string())),
                Err(TryRecvError::Empty) => None,
            }
        });
        if let Some(result) = finished {
            self.an.job = None;
            match result {
                Ok(path) => self.say(ctx, format!("Saved {path}")),
                Err(error) => self.say(ctx, format!("GIF not saved: {error}")),
            }
        }
    }

    /// A draft owns its source shapes until it is finished or cancelled. Idle
    /// curve/pipe tools can still use the timeline; an actual edit cannot.
    pub(crate) fn animation_edit_active(&self) -> bool {
        self.scene_structure_locked()
            || self.sketch_active()
            || self.drag.is_some()
            || self.ct.drag.is_some()
            || self.ct.draw_shape.is_some()
            || self.pt.tracing
    }

    /// A control command already applied its explicit result on the window
    /// thread. Keep it out of UI auto-capture; `anim.at` also moves the playhead
    /// when the scene's values happened to be unchanged.
    pub(crate) fn animation_at_applied(&mut self, frame: u32) {
        self.an.playing = false;
        self.an.frame = frame.min(self.scene.anim.length.saturating_sub(1));
        self.an.clock = self.an.frame as f32;
        self.an.rev = self.track_scene();
    }

    /// Put the scene into its pose at `frame` without making an undo step.
    fn show_pose(&mut self, frame: u32) {
        if self.animation_edit_active() {
            return;
        }
        let frame = frame.min(self.scene.anim.length.saturating_sub(1));
        let anim = std::mem::take(&mut self.scene.anim);
        anim.apply(&mut self.scene, frame);
        self.scene.anim = anim;
        self.an.frame = frame;
        self.committed_sync();
        self.an.rev = self.track_scene();
    }

    fn capture_animation_pose(&mut self, ctx: &egui::Context) {
        if self.animation_edit_active() {
            return;
        }
        let mut anim = std::mem::take(&mut self.scene.anim);
        let result = anim.capture(&mut self.scene, self.an.frame);
        self.scene.anim = anim;
        if let Err(e) = result {
            self.say(ctx, e);
        }
        self.an.rev = self.track_scene();
    }

    /// Run after the viewport/cards finish their gestures and before `settle`:
    /// the finished edit and its pose then form the same undo step.
    pub(crate) fn animation_capture_finished(&mut self, ctx: &egui::Context) {
        if !self.an.open || self.an.playing || self.animation_edit_active() || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let rev = self.track_scene();
        if rev != self.an.rev {
            self.an.rev = rev;
            if !self.scene.anim.is_empty() && self.scene.anim.differs(&self.scene, self.an.frame) {
                self.capture_animation_pose(ctx);
            }
        }
    }

    pub(crate) fn anim_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // a GIF being made: its end is noticed whether the timeline is open or not
        self.poll_gif_job(&ctx);

        if !self.an.open {
            self.an.playing = false;
            let mut reopen_timeline = false;
            if let Some(job) = &self.an.job {
                egui::Panel::bottom("gif_background_progress").exact_size(32.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let done = job.done.load(Ordering::Relaxed);
                        let total = job.total.load(Ordering::Relaxed);
                        ui.label(format!("Making GIF {done}/{total}"));
                        if ui.button("Cancel GIF").clicked() {
                            job.cancel.store(true, Ordering::Relaxed);
                        }
                        if ui.button("Show timeline").clicked() {
                            reopen_timeline = true;
                        }
                    });
                });
            }
            if reopen_timeline {
                self.an.open = true;
            }
            return;
        }
        let length = self.scene.anim.length.max(1);
        self.an.frame = self.an.frame.min(length - 1);
        let editing = self.animation_edit_active();
        if editing {
            self.an.playing = false;
        }
        // playing
        if self.an.playing {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            self.an.clock += dt * self.scene.anim.fps as f32;
            let f = (self.an.clock as u32) % length;
            if f != self.an.frame || self.an.clock < 1.0 {
                self.show_pose(f);
            }
            ctx.request_repaint();
        }

        let mut go: Option<u32> = None;
        let mut pose_now = false;
        let mut delete_now = false;
        let mut start_gif = false;
        egui::Panel::bottom("anim_bar")
            .exact_size(62.0)
            .frame(egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(12, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_enabled(!editing, egui::Button::new(if self.an.playing { "Stop" } else { "Play" }))
                        .on_hover_text("Play the animation (Space)")
                        .on_disabled_hover_text("Finish or cancel the sketch or drag before playing")
                        .clicked()
                    {
                        self.an.playing = !self.an.playing;
                        self.an.clock = self.an.frame as f32;
                    }
                    if ui.add_enabled(!editing, egui::Button::new("|<")).on_hover_text("Back to the first frame").clicked() {
                        self.an.playing = false;
                        go = Some(0);
                    }
                    let has_pose = self.scene.anim.key_frames().contains(&self.an.frame);
                    if ui
                        .add_enabled(!editing, egui::Button::new(RichText::new("Set pose").color(style::ACCENT)))
                        .on_hover_text("Keep the scene as it is now at this frame")
                        .on_disabled_hover_text("Finish or cancel the sketch or drag before recording a pose")
                        .clicked()
                    {
                        pose_now = true;
                    }
                    if ui.add_enabled(!editing && has_pose, egui::Button::new("Delete pose")).on_hover_text("Forget the pose at this frame").clicked()
                    {
                        delete_now = true;
                    }
                    ui.label(RichText::new(format!("Frame {} / {}", self.an.frame + 1, length)).size(12.0).color(style::DIM));
                    ui.separator();
                    let a = &mut self.scene.anim;
                    // Shortening below the last pose would squash poses together; delete them first.
                    let shortest = a.key_frames().last().map_or(2, |l| (l + 1).max(2));
                    ui.label(RichText::new("Speed").size(12.0).color(style::DIM));
                    ui.add_enabled(!editing, egui::DragValue::new(&mut a.fps).range(1..=60).suffix(" fps"));
                    ui.label(RichText::new("Length").size(12.0).color(style::DIM));
                    ui.add_enabled(!editing, egui::DragValue::new(&mut a.length).range(shortest..=crate::anim::MAX_FRAMES).suffix(" frames"));
                    ui.add_enabled(!editing, egui::Checkbox::new(&mut a.clay, "Clay"))
                        .on_hover_text("Hold each pose for a few frames, like stop motion");
                    ui.separator();
                    if let Some(job) = &self.an.job {
                        let (d, t) = (job.done.load(Ordering::Relaxed), job.total.load(Ordering::Relaxed));
                        ui.label(RichText::new(format!("Making GIF {d}/{t}")).size(12.0).color(style::DIM));
                        if ui.button("Cancel").clicked() {
                            job.cancel.store(true, Ordering::Relaxed);
                        }
                    } else if ui
                        .add_enabled(!self.scene.anim.is_empty(), egui::Button::new("Export GIF"))
                        .on_hover_text("Save the animation as a moving picture")
                        .clicked()
                    {
                        start_gif = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.an.open = false;
                            self.an.playing = false;
                        }
                    });
                });
                // the timeline: poses are diamonds, the playhead is the green line
                let w = ui.available_width();
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), if editing { Sense::hover() } else { Sense::click_and_drag() });
                let p = ui.painter_at(rect);
                p.rect_filled(rect.shrink2(vec2(0.0, 8.0)), 3.0, style::CARD);
                let x_of = |f: u32| rect.left() + 8.0 + (rect.width() - 16.0) * (f as f32 / (length - 1).max(1) as f32);
                for f in self.scene.anim.key_frames() {
                    let c = pos2(x_of(f), rect.center().y);
                    p.add(egui::Shape::convex_polygon(
                        vec![c + vec2(0.0, -6.0), c + vec2(5.0, 0.0), c + vec2(0.0, 6.0), c + vec2(-5.0, 0.0)],
                        style::KEEP,
                        egui::Stroke::NONE,
                    ));
                }
                let x = x_of(self.an.frame);
                p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], egui::Stroke::new(2.0, style::ACCENT));
                if !editing
                    && let Some(pos) = resp.interact_pointer_pos()
                    && (resp.clicked() || resp.dragged())
                {
                    let t = ((pos.x - rect.left() - 8.0) / (rect.width() - 16.0).max(1.0)).clamp(0.0, 1.0);
                    go = Some((t * (length - 1) as f32).round() as u32);
                    self.an.playing = false;
                }
            });
        if let Some(f) = go
            && (f != self.an.frame || !self.scene.anim.is_empty())
        {
            self.show_pose(f);
        }
        if pose_now {
            self.capture_animation_pose(&ctx);
        }
        if delete_now {
            self.scene.anim.remove(self.an.frame);
        }
        if start_gif {
            self.start_gif(&ctx);
        }
    }

    fn start_gif(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("GIF", &["gif"]).set_file_name("Melt3D.gif").save_file() else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        let (done, total, cancel) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(1)), Arc::new(AtomicBool::new(false)));
        let (d2, t2, c2) = (done.clone(), total.clone(), cancel.clone());
        let (scene, cam) = (self.scene.clone(), self.cam);
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let _guard = crate::mesh::LoadGuard::begin();
            let r = crate::cpu_render::animation_gif(&scene, Some(cam), 480, true, |i, n| {
                d2.store(i, Ordering::Relaxed);
                t2.store(n, Ordering::Relaxed);
                !c2.load(Ordering::Relaxed)
            })
            .and_then(|bytes| save_gif_if_active(&path, bytes, &c2));
            let _ = tx.send(r);
            wake.request_repaint();
        });
        self.an.job = Some(Job { rx, done, total, cancel });
    }
}

#[cfg(test)]
#[path = "anim_ui_tests.rs"]
mod tests;
