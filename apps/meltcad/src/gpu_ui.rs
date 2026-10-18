//! The window's side of the graphics path: which picture the viewport shows (the GPU one or the
//! simple CPU one), the card that says what is going on while the shader compiles or when it
//! failed, and the 'がしつ' menu entries (simple view, backend, log).

use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

use egui::{Align2, RichText, Ui};

use crate::app::App;
use crate::backend::{Backend, Choice, Relaunch};
use crate::gpu_status::{self, GpuStatus, OFFER_WAYS_OUT_AFTER_SECS, Snapshot, State};
use crate::style;

/// The graphics path as the window holds it.
#[derive(Default)]
pub struct GpuUi {
    pub status: GpuStatus,
    /// The device and renderer of the window (None: the window could not get a graphics device).
    pub rs: Option<egui_wgpu::RenderState>,
    /// The person chose the simple view: the GPU picture is not used (and not even built).
    pub easy: bool,
    /// The failure card was asked for while the simple view is chosen.
    pub details: bool,
    pub easy_view: crate::easy_view::EasyView,
    /// A line for the person after a button (a restart that could not be done, a copy).
    pub note: Option<String>,
}

/// Which picture the viewport shows.
pub fn use_gpu(snap: &Snapshot, easy: bool) -> bool {
    snap.is_ready() && !easy
}

/// Which card goes over the viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Card {
    None,
    /// "Preparing graphics…" (with the ways out after a while).
    Compiling {
        secs: u64,
        ways_out: bool,
    },
    /// The big card with the error and the buttons.
    Failed,
    /// A thin line: the simple view is used because the graphics path failed.
    FailedPill,
    /// A small tag: the simple view is chosen.
    EasyTag,
}

pub fn card_for(snap: &Snapshot, easy: bool, details: bool, now: Instant) -> Card {
    match (&snap.state, easy) {
        (State::Ready { .. }, false) => Card::None,
        (State::Off, false) => Card::None,
        (State::Compiling { .. }, false) => {
            let secs = snap.compiling_for(now).map_or(0, |d| d.as_secs());
            Card::Compiling { secs, ways_out: secs >= OFFER_WAYS_OUT_AFTER_SECS }
        }
        (State::Failed { .. }, false) => Card::Failed,
        (State::Failed { .. }, true) => {
            if details {
                Card::Failed
            } else {
                Card::FailedPill
            }
        }
        (_, true) => Card::EasyTag,
    }
}

/// What the "Copy log" button puts on the clipboard.
pub fn report_text(snap: &Snapshot, log_tail: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("MELTCAD {} / {}\n", env!("CARGO_PKG_VERSION"), snap.info.short()));
    if !snap.info.driver.is_empty() {
        out.push_str(&format!("driver: {}\n", snap.info.driver));
    }
    match &snap.state {
        State::Failed { message } => out.push_str(&format!("Status: failed\n{message}\n")),
        State::Compiling { .. } => out.push_str("Status: preparing\n"),
        State::Ready { secs } => out.push_str(&format!("Status: done ({secs:.1} s)\n")),
        State::Off => out.push_str("Status: not in use\n"),
    }
    if let Some(e) = &snap.last_error {
        out.push_str(&format!("Last error: {e}\n"));
    }
    out.push_str("---- log ----\n");
    out.push_str(log_tail);
    out.push('\n');
    out
}

/// The program and arguments that open a file with the system's default application.
pub fn open_command(os: &str, path: &Path) -> (String, Vec<OsString>) {
    match os {
        "windows" => ("cmd".into(), vec!["/C".into(), "start".into(), "".into(), path.as_os_str().to_owned()]),
        "macos" => ("open".into(), vec![path.as_os_str().to_owned()]),
        _ => ("xdg-open".into(), vec![path.as_os_str().to_owned()]),
    }
}

fn open_file(path: &Path) -> Result<(), String> {
    let (program, args) = open_command(std::env::consts::OS, path);
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // (no flashing console window)
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Could not open: {e}"))
}

impl App {
    /// Give the app the window's graphics device. `easy` is the person's saved choice of the
    /// simple view. Starts building the GPU picture unless the simple view is chosen.
    pub fn attach_gpu(&mut self, rs: Option<egui_wgpu::RenderState>, ctx: &egui::Context, easy: bool) {
        self.gpu.easy = easy;
        self.gpu.rs = rs;
        match &self.gpu.rs {
            None => {
                let g = self.gpu.status.begin(Instant::now());
                self.gpu.status.finish_err(g, "Could not create a graphics device (no wgpu render state). See the log for details.");
            }
            Some(rs) if !easy => crate::gpu::install(rs, ctx, &self.gpu.status),
            Some(rs) => {
                // (still describe the adapter: the menu and the log say what would be used)
                self.gpu.status.set_info(crate::gpu_status::AdapterInfo::from_wgpu(&rs.adapter.get_info()));
                crate::gpu_log::log("Simple view is on, so GPU setup is skipped");
                crate::gpu_log::log_adapter(&rs.adapter);
            }
        }
    }

    /// Switch the simple view on or off (and remember it).
    pub fn set_easy(&mut self, ctx: &egui::Context, on: bool) {
        self.gpu.easy = on;
        self.gpu.details = false;
        crate::gpu_log::log(&format!("Simple view: {}", if on { "ON" } else { "OFF" }));
        // back to the GPU picture: build it if it was never started
        if !on && matches!(self.gpu.status.snapshot().state, State::Off) {
            self.retry_gpu(ctx);
        }
        ctx.request_repaint();
    }

    /// Try the GPU picture again.
    pub fn retry_gpu(&mut self, ctx: &egui::Context) {
        if let Some(rs) = self.gpu.rs.clone() {
            crate::gpu_log::log("retry: building the graphics path again");
            crate::gpu::install(&rs, ctx, &self.gpu.status);
        }
        ctx.request_repaint();
    }

    /// The backend the window runs on.
    pub fn current_backend(&self) -> Option<Backend> {
        Backend::parse(&self.gpu.status.snapshot().info.backend)
    }

    /// Unsaved work stops a restart (the work would be lost).
    pub fn restart_blocked(&self) -> bool {
        self.dirty
    }

    /// Start the program again on another backend (`None` = let it choose) and close this window.
    pub fn restart_with(&mut self, ctx: &egui::Context, choice: Option<Backend>, current_failed: bool) {
        if self.restart_blocked() {
            self.gpu.note = Some("You have unsaved changes. Save first (Ctrl+S).".into());
            return;
        }
        let current = self.current_backend();
        if current_failed && let Some(b) = current {
            crate::backend::remember(b, false);
        }
        if choice.is_none() {
            // "auto" forgets what was learned
            crate::backend::update(|s| {
                s.last_ok = None;
                s.failed.clear();
            });
        }
        let r = Relaunch { choice: choice.map(|b| b.name().to_string()), tried: Vec::new(), file: self.path.clone() };
        match crate::backend::relaunch(&r) {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => {
                crate::gpu_log::log(&format!("restart failed: {e}"));
                self.gpu.note = Some(e);
            }
        }
    }

    /// The next backend `auto` would try after this one failed (None: nothing is left).
    pub fn next_backend(&self) -> Option<Backend> {
        let mut settings = crate::backend::load();
        let mut tried = crate::backend::tried_now();
        if let Some(b) = self.current_backend() {
            settings.note_failed(b);
            if !tried.contains(&b) {
                tried.push(b);
            }
        }
        let windows = cfg!(target_os = "windows");
        match crate::backend::plan(Choice::Auto, windows, &settings, &tried) {
            Some(crate::backend::Plan::Only(b)) => Some(b),
            // (elsewhere the defaults have no order: offer the other backends of the platform)
            _ => Backend::offered(windows, cfg!(target_os = "macos")).into_iter().find(|b| !tried.contains(b)),
        }
    }

    // ----- the cards -----

    /// Draw the card for the state of the graphics path over the viewport `rect`.
    pub fn gpu_cards(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let snap = self.gpu.status.snapshot();
        let card = card_for(&snap, self.gpu.easy, self.gpu.details, Instant::now());
        match &card {
            Card::None => {}
            Card::Compiling { secs, ways_out } => {
                // the seconds count up
                ctx.request_repaint_after(std::time::Duration::from_millis(1000));
                self.compiling_card(ctx, rect, &snap, *secs, *ways_out);
            }
            Card::Failed => self.failed_card(ctx, rect, &snap),
            Card::FailedPill => self.pill_card(ctx, rect),
            Card::EasyTag => {
                egui::Area::new(egui::Id::new("easy_tag"))
                    .order(egui::Order::Middle)
                    .pivot(Align2::LEFT_BOTTOM)
                    .fixed_pos(rect.left_bottom() + egui::vec2(14.0, -12.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.label(RichText::new("Simple view").size(12.0).color(style::DIM));
                    });
            }
        }
    }

    fn compiling_card(&mut self, ctx: &egui::Context, rect: egui::Rect, snap: &Snapshot, secs: u64, ways_out: bool) {
        egui::Area::new(egui::Id::new("gpu_compiling"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(rect.center().x, rect.top() + 16.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_width(430.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(16.0).color(style::ACCENT));
                        ui.label(RichText::new(gpu_status::compiling_text(secs)).size(13.5).color(style::TEXT));
                    });
                    ui.add_space(2.0);
                    ui.label(RichText::new(format!("Using: {}", snap.info.short())).size(12.0).color(style::DIM));
                    ui.label(
                        RichText::new("Meanwhile you can work in Simple view. It switches over by itself when ready.").size(12.0).color(style::DIM),
                    );
                    if ways_out {
                        ui.add_space(4.0);
                        ui.label(RichText::new("If this takes too long, try a button below.").size(12.0).color(style::KEEP));
                        self.way_out_buttons(ui, ctx, false);
                    }
                    self.note_line(ui);
                });
            });
    }

    fn way_out_buttons(&mut self, ui: &mut Ui, ctx: &egui::Context, with_retry: bool) {
        ui.horizontal_wrapped(|ui| {
            if with_retry && style::pill(ui, "Try again", false).on_hover_text("Start the graphics setup again").clicked() {
                self.retry_gpu(ctx);
            }
            let next = self.next_backend();
            let blocked = self.restart_blocked();
            let label = match next {
                Some(b) => format!("Restart with another graphics mode ({})", b.label()),
                None => "Restart with another graphics mode".to_string(),
            };
            let tip = if blocked {
                "You have unsaved changes. Save first (Ctrl+S)"
            } else {
                "Restarts the app and uses another graphics mode (Vulkan, DirectX 12, OpenGL)"
            };
            if style::pill_enabled(ui, &label, false, next.is_some() && !blocked).on_hover_text(tip).clicked()
                && let Some(b) = next
            {
                self.restart_with(ctx, Some(b), true);
            }
            if style::pill(ui, "Use Simple view", false).on_hover_text("Draw small on the CPU, without the GPU").clicked() {
                self.set_easy(ctx, true);
            }
        });
    }

    fn note_line(&mut self, ui: &mut Ui) {
        if let Some(n) = &self.gpu.note {
            ui.add_space(4.0);
            ui.label(RichText::new(n).size(12.0).color(style::CUT));
        }
    }

    fn failed_card(&mut self, ctx: &egui::Context, rect: egui::Rect, snap: &Snapshot) {
        let message = match &snap.state {
            State::Failed { message } => message.clone(),
            _ => String::new(),
        };
        let log_path = crate::gpu_log::path();
        egui::Area::new(egui::Id::new("gpu_failed")).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(rect.center()).show(
            ctx,
            |ui| {
                style::card().show(ui, |ui| {
                    ui.set_width(520.0);
                    ui.label(RichText::new("Graphics setup failed").strong().size(15.0).color(style::CUT));
                    ui.add_space(2.0);
                    ui.label(RichText::new("You are seeing Simple view for now. You can keep working.").size(12.5).color(style::TEXT));
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("Using: {}", snap.info.short())).size(12.0).color(style::DIM));
                    if !snap.info.driver.is_empty() {
                        ui.label(RichText::new(format!("Driver: {}", snap.info.driver)).size(12.0).color(style::DIM));
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Error details (select to copy)").size(12.0).color(style::DIM));
                    egui::ScrollArea::vertical().max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                        let mut text: &str = &message;
                        ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(3));
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Log: {}", log_path.display())).size(12.0).color(style::DIM));
                    ui.add_space(6.0);
                    self.way_out_buttons(ui, ctx, true);
                    ui.horizontal_wrapped(|ui| {
                        if style::pill(ui, "Open log", false).clicked()
                            && let Err(e) = open_file(&log_path)
                        {
                            self.gpu.note = Some(e);
                        }
                        if style::pill(ui, "Copy log", false)
                            .on_hover_text("Copies the error and the end of the log. Paste it when you send it.")
                            .clicked()
                        {
                            ctx.copy_text(report_text(snap, &crate::gpu_log::global().tail(200)));
                            self.gpu.note = Some("Copied.".into());
                        }
                        if self.gpu.easy && style::pill(ui, "Close", false).clicked() {
                            self.gpu.details = false;
                        }
                    });
                    if self.restart_blocked() {
                        ui.label(RichText::new("You have unsaved changes. Save first, then restart (Ctrl+S).").size(12.0).color(style::KEEP));
                    }
                    self.note_line(ui);
                });
            },
        );
    }

    fn pill_card(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        egui::Area::new(egui::Id::new("gpu_pill"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(rect.center().x, rect.top() + 12.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Simple view (GPU drawing unavailable)").size(12.5).color(style::TEXT));
                        if style::pill(ui, "Details", false).clicked() {
                            self.gpu.details = true;
                        }
                    });
                });
            });
    }

    // ----- the menu -----

    /// The graphics entries of the 'がしつ' menu.
    pub fn gpu_menu_items(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.separator();
        let mut easy = self.gpu.easy;
        if ui
            .checkbox(&mut easy, "Simple view (no GPU)")
            .on_hover_text("Draws small on the CPU. Use it when the screen stays blank or drawing is slow.")
            .changed()
        {
            self.set_easy(&ctx, easy);
        }
        let snap = self.gpu.status.snapshot();
        let state = match &snap.state {
            State::Off => "not in use",
            State::Compiling { .. } => "preparing…",
            State::Ready { .. } => "ready",
            State::Failed { .. } => "unavailable",
        };
        ui.label(RichText::new(format!("Graphics: {} ({state})", snap.info.short())).size(12.0).color(style::DIM));
        if let State::Failed { .. } = snap.state
            && ui.button("See what failed").clicked()
        {
            self.gpu.details = true;
            ui.close();
        }
        ui.menu_button("Restart with a different graphics mode", |ui| {
            let blocked = self.restart_blocked();
            if blocked {
                ui.label(RichText::new("Save first (Ctrl+S)").size(12.0).color(style::KEEP));
            }
            ui.add_enabled_ui(!blocked, |ui| {
                if ui.button("Auto (recommended)").clicked() {
                    self.restart_with(&ctx, None, false);
                    ui.close();
                }
                let windows = cfg!(target_os = "windows");
                let macos = cfg!(target_os = "macos");
                for b in Backend::offered(windows, macos) {
                    let here = self.current_backend() == Some(b);
                    if ui.selectable_label(here, format!("{}{}", b.label(), if here { "(now)" } else { "" })).clicked() {
                        self.restart_with(&ctx, Some(b), false);
                        ui.close();
                    }
                }
            });
        });
        if ui.button("Open log").on_hover_text(crate::gpu_log::path().display().to_string()).clicked() {
            if let Err(e) = open_file(&crate::gpu_log::path()) {
                self.gpu.note = Some(e);
            }
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_status::AdapterInfo;
    use std::time::Duration;

    fn snap(state: State) -> Snapshot {
        Snapshot {
            state,
            info: AdapterInfo { backend: "Dx12".into(), name: "Test GPU".into(), driver: "d 1".into(), device_type: "DiscreteGpu".into() },
            runtime_errors: 0,
            last_error: None,
        }
    }

    #[test]
    fn only_a_ready_path_that_is_not_switched_off_draws_on_the_gpu() {
        let ready = snap(State::Ready { secs: 1.0 });
        assert!(use_gpu(&ready, false));
        assert!(!use_gpu(&ready, true), "the person chose the simple view");
        let now = Instant::now();
        assert!(!use_gpu(&snap(State::Compiling { since: now }), false));
        assert!(!use_gpu(&snap(State::Failed { message: "x".into() }), false));
        assert!(!use_gpu(&snap(State::Off), false));
    }

    #[test]
    fn the_card_follows_the_state() {
        let now = Instant::now();
        let later = now + Duration::from_secs(12);
        let compiling = snap(State::Compiling { since: now });
        assert_eq!(card_for(&compiling, false, false, later), Card::Compiling { secs: 12, ways_out: false });
        assert_eq!(card_for(&compiling, false, false, now + Duration::from_secs(20)), Card::Compiling { secs: 20, ways_out: true });
        assert_eq!(card_for(&snap(State::Ready { secs: 1.0 }), false, false, now), Card::None);
        assert_eq!(card_for(&snap(State::Off), false, false, now), Card::None);
        let failed = snap(State::Failed { message: "boom".into() });
        assert_eq!(card_for(&failed, false, false, now), Card::Failed);
        assert_eq!(card_for(&failed, true, false, now), Card::FailedPill, "the chosen simple view only keeps a thin line");
        assert_eq!(card_for(&failed, true, true, now), Card::Failed, "...which opens the details");
        assert_eq!(card_for(&compiling, true, false, later), Card::EasyTag);
        assert_eq!(card_for(&snap(State::Ready { secs: 1.0 }), true, false, now), Card::EasyTag);
    }

    #[test]
    fn the_report_has_what_we_need_to_help() {
        let mut s = snap(State::Failed { message: "pipeline: X3500 unable to unroll loop".into() });
        s.last_error = Some("late error".into());
        let t = report_text(&s, "line one\nline two");
        assert!(t.contains("Dx12 / Test GPU"), "{t}");
        assert!(t.contains("driver: d 1"));
        assert!(t.contains("X3500 unable to unroll loop"));
        assert!(t.contains("late error"));
        assert!(t.ends_with("line one\nline two\n"));
        let ok = report_text(&snap(State::Ready { secs: 2.5 }), "");
        assert!(ok.contains("2.5"), "{ok}");
    }

    #[test]
    fn opening_the_log_uses_the_systems_opener() {
        let p = Path::new("C:\\a b\\meltcad-log.txt");
        let (prog, args) = open_command("windows", p);
        assert_eq!(prog, "cmd");
        assert_eq!(args.last(), Some(&p.as_os_str().to_owned()));
        assert!(args.iter().any(|a| a == "start"));
        assert_eq!(open_command("macos", p).0, "open");
        assert_eq!(open_command("linux", p).0, "xdg-open");
    }

    #[test]
    fn a_window_without_a_graphics_device_says_so() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.attach_gpu(None, &ctx, false);
        match app.gpu.status.snapshot().state {
            State::Failed { message } => assert!(message.contains("device"), "{message}"),
            other => panic!("{other:?}"),
        }
        assert!(!use_gpu(&app.gpu.status.snapshot(), false));
    }

    #[test]
    fn a_restart_with_unsaved_work_is_refused_with_a_reason() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.dirty = true;
        app.restart_with(&ctx, Some(Backend::Gl), true);
        let note = app.gpu.note.clone().unwrap_or_default();
        assert!(note.contains("Save"), "{note}");
    }
}
