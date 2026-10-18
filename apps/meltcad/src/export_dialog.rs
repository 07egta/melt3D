//! The export card: file type, precision (the size of a mesh cell in mm) with a live estimate,
//! units, size, which part to export, smoothing, lighter files, colours, and a print check. The
//! numbers live in egui's memory like the other cards. Exporting runs on a worker thread; a small
//! card with a bar and a cancel button shows how far it is.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, RichText, Stroke};

use crate::app::{App, ExportFormat};
use crate::export_formats::{Format, Units};
use crate::export_job::{self, Estimate, Job, Lighten, MAX_TOLERANCE, MIN_TOLERANCE, Options, PrintCheck, Scope, Up};
use crate::export_mesh::{CANCELLED, MAX_VERTS, Progress};
use crate::model::{Pick, Scene};
use crate::style;

// ---------------------------------------------------------------- small background tasks

enum Slot<T> {
    Idle,
    Running,
    Done(Result<T, String>),
}

/// Something computed on another thread; clones share the answer.
#[derive(Clone)]
struct Task<T> {
    slot: Arc<Mutex<Slot<T>>>,
    prog: Arc<Progress>,
}

impl<T: Clone + Send + 'static> Task<T> {
    fn idle() -> Task<T> {
        Task { slot: Arc::new(Mutex::new(Slot::Idle)), prog: Arc::new(Progress::new()) }
    }
    fn start(&mut self, f: impl FnOnce(&Progress) -> Result<T, String> + Send + 'static) {
        self.prog.cancel();
        let slot = Arc::new(Mutex::new(Slot::Running));
        let prog = Arc::new(Progress::new());
        self.slot = slot.clone();
        self.prog = prog.clone();
        let spawned = std::thread::Builder::new().name("yura-export-task".into()).spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&prog))).unwrap_or_else(|_| Err("stopped unexpectedly".into()));
            if let Ok(mut s) = slot.lock() {
                *s = Slot::Done(res);
            }
        });
        if spawned.is_err()
            && let Ok(mut s) = self.slot.lock()
        {
            *s = Slot::Done(Err("could not start".into()));
        }
    }
    fn reset(&mut self) {
        self.prog.cancel();
        self.slot = Arc::new(Mutex::new(Slot::Idle));
        self.prog = Arc::new(Progress::new());
    }
    fn running(&self) -> bool {
        self.slot.lock().is_ok_and(|s| matches!(*s, Slot::Running))
    }
    fn result(&self) -> Option<Result<T, String>> {
        match &*self.slot.lock().ok()? {
            Slot::Done(r) => Some(r.clone()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- state

#[derive(Clone)]
struct Dlg {
    opts: Options,
    /// What the estimate and the check were made for.
    snap: Option<(Scene, Option<Pick>, Scope, bool)>,
    est: Task<Estimate>,
    check: Task<PrintCheck>,
    /// The check should start by itself on the next frame.
    run_check: bool,
    /// Show technical export options only when needed.
    advanced: bool,
    msg: String,
    /// Is the card showing? (`remove_temp` leaves a default value behind, which is not open.)
    open: bool,
}

fn id() -> egui::Id {
    egui::Id::new("export_card")
}

fn last_id() -> egui::Id {
    egui::Id::new("export_last_options")
}

fn job_id() -> egui::Id {
    egui::Id::new("export_running_job")
}

/// The running export (shared so that egui's memory can hold it).
#[derive(Clone, Default)]
struct Running(Option<Arc<Job>>);

fn last(ctx: &egui::Context) -> Option<Options> {
    ctx.data(|m| m.get_temp::<Options>(last_id()))
}

fn is_running(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<Running>(job_id())).is_some_and(|r| r.0.is_some())
}

impl Default for Dlg {
    fn default() -> Dlg {
        Dlg {
            opts: Options::default(),
            snap: None,
            est: Task::idle(),
            check: Task::idle(),
            run_check: false,
            advanced: false,
            msg: String::new(),
            open: false,
        }
    }
}

fn new_dlg(ctx: &egui::Context, format: Option<Format>) -> Dlg {
    let mut opts = last(ctx).unwrap_or_default();
    if let Some(f) = format {
        opts.format = f;
        opts.up = if f.y_up_by_default() { Up::Y } else { Up::Z };
    }
    let advanced = has_custom_settings(&opts);
    Dlg { opts, advanced, open: true, ..Dlg::default() }
}

/// Open the card (optionally with a file type chosen).
pub fn open(ctx: &egui::Context, format: Option<ExportFormat>) {
    let f = format.map(|f| match f {
        ExportFormat::Stl => Format::Stl,
        ExportFormat::ThreeMf => Format::ThreeMf,
        ExportFormat::Obj => Format::Obj,
    });
    let d = new_dlg(ctx, f);
    ctx.data_mut(|m| m.insert_temp(id(), d));
}

/// Open the card and measure the model for printing.
pub fn open_check(ctx: &egui::Context) {
    let mut d = new_dlg(ctx, None);
    d.run_check = true;
    ctx.data_mut(|m| m.insert_temp(id(), d));
}

/// Ctrl+E: export again with the last settings (the card opens the first time).
pub fn quick(app: &mut App, ctx: &egui::Context) {
    if is_running(ctx) {
        app.say(ctx, "Export is already running");
        return;
    }
    match last(ctx) {
        Some(o) => {
            start_with(app, ctx, o);
        }
        None => open(ctx, None),
    }
}

// ---------------------------------------------------------------- words

fn jp_error(e: &str) -> String {
    if e == CANCELLED {
        "Cancelled".into()
    } else if e.contains("too big for this precision") || e.contains("too fine for this model") || e.contains("too many triangles") {
        "Too big at this detail level. Try a coarser setting".into()
    } else if e.contains("nothing to export") || e.contains("shapes are empty") || e.contains("no size") {
        "Nothing to export".into()
    } else if e.contains("nothing is selected") || e.contains("selection is gone") {
        "Select shapes first, then export".into()
    } else {
        format!("Export failed ({e})")
    }
}

fn count_text(n: f64) -> String {
    if n >= 1.0e8 {
        format!("{:.1} x100M", n / 1.0e8)
    } else if n >= 1.0e4 {
        format!("{:.0} x10K", n / 1.0e4)
    } else {
        format!("{:.0}", n)
    }
}

fn time_text(s: f64) -> String {
    if s < 1.5 {
        "about 1 sec".into()
    } else if s < 90.0 {
        format!("about {:.0} sec", s)
    } else if s < 5400.0 {
        format!("about {:.0} min", s / 60.0)
    } else {
        format!("about {:.1} h", s / 3600.0)
    }
}

fn bytes_text(b: f64) -> String {
    if b >= 1.0e9 {
        format!("{:.1}GB", b / 1.0e9)
    } else if b >= 1.0e6 {
        format!("{:.0}MB", b / 1.0e6)
    } else {
        format!("{:.0}KB", (b / 1.0e3).max(1.0))
    }
}

/// About how many bytes a triangle costs in each file type.
fn bytes_per_tri(f: Format) -> f64 {
    match f {
        Format::Stl => 50.0,
        Format::StlAscii => 255.0,
        Format::Obj => 48.0,
        Format::ThreeMf => 14.0,
        Format::Ply => 21.0,
        Format::Glb => 18.0,
    }
}

fn default_name(app: &App, f: Format) -> String {
    let stem = app
        .path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "design".into());
    format!("{stem}.{}", f.ext())
}

/// Change the file type without unexpectedly resetting scope, scale or quality.
fn choose_format(o: &mut Options, format: Format) {
    o.format = format;
    o.up = if format.y_up_by_default() { Up::Y } else { Up::Z };
}

fn quality_preset(t: f32) -> Option<usize> {
    [0.2_f32, 0.1, 0.5].iter().position(|v| (t - v).abs() < 1e-4)
}

/// Reveal saved expert choices to avoid silently exporting at an unexpected scale.
fn has_custom_settings(o: &Options) -> bool {
    !matches!(o.format, Format::Stl | Format::ThreeMf | Format::Glb)
        || quality_preset(o.tolerance).is_none()
        || o.units != Units::Mm
        || (o.scale_pct - 100.0).abs() > 0.01
        || o.up != (if o.format.y_up_by_default() { Up::Y } else { Up::Z })
        || o.smooth != 0
        || o.lighten != Lighten::Flat
        || o.ignore_paint
        || !o.colour
}

// ---------------------------------------------------------------- running

/// Ask where to save, then start the export. Returns whether it started.
fn start_with(app: &mut App, ctx: &egui::Context, o: Options) -> bool {
    let o = o.sanitized();
    if o.scope == Scope::Selected && app.sel.is_none() {
        app.say(ctx, "Select shapes first, then export");
        return false;
    }
    let Some(mut path) =
        rfd::FileDialog::new().add_filter(o.format.label(), &[o.format.ext()]).set_file_name(default_name(app, o.format)).save_file()
    else {
        return false;
    };
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(o.format.ext())) {
        path.set_extension(o.format.ext());
    }
    start_job(app, ctx, o, path)
}

fn start_job(app: &mut App, ctx: &egui::Context, o: Options, path: PathBuf) -> bool {
    if is_running(ctx) {
        app.say(ctx, "Export is already running");
        return false;
    }
    let job = Job::start(app.scene.clone(), app.sel, o.clone(), path);
    ctx.data_mut(|m| {
        m.insert_temp(job_id(), Running(Some(Arc::new(job))));
        m.insert_temp(last_id(), o);
    });
    ctx.request_repaint();
    true
}

fn size_text(s: [f64; 3]) -> String {
    format!("{:.1} × {:.1} × {:.1} mm", s[0], s[1], s[2])
}

/// The progress card, and the end of a running export.
fn running_card(app: &mut App, ctx: &egui::Context) {
    let Some(Running(Some(job))) = ctx.data(|m| m.get_temp::<Running>(job_id())) else { return };
    if let Some(res) = job.poll() {
        ctx.data_mut(|m| m.insert_temp(job_id(), Running(None)));
        match res {
            Ok(r) => {
                let closed = if r.closed { "" } else { " ⚠ Has holes" };
                app.say(
                    ctx,
                    format!(
                        "Exported: {} (triangles {} · {} · {:.1} sec){closed}",
                        job.label,
                        count_text(r.tris as f64),
                        bytes_text(r.bytes as f64),
                        r.secs
                    ),
                );
            }
            Err(e) => app.say(ctx, jp_error(&e)),
        }
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(60));
    egui::Area::new(egui::Id::new("export_progress_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, [0.0, -48.0]).show(
        ctx,
        |ui| {
            style::card().show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new(format!("Exporting… {}", job.label)).strong().size(13.5));
                ui.label(RichText::new(job.phase_name()).size(11.5).color(style::DIM));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 5.0, style::PANEL_DARK);
                let w = rect.width() * job.fraction().clamp(0.0, 1.0);
                if w > 1.0 {
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height())), 5.0, style::ACCENT);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:.0}%  {:.0} sec", job.fraction() * 100.0, job.elapsed())).size(11.5).color(style::DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if style::pill(ui, "Cancel", false).clicked() {
                            job.cancel();
                        }
                    });
                });
            });
        },
    );
}

// ---------------------------------------------------------------- the card

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        style::row_label(ui, label);
        add(ui);
    });
}

/// Call once per frame: the progress of a running export and the card when it is open.
pub fn update(app: &mut App, ctx: &egui::Context) {
    running_card(app, ctx);
    let Some(mut d) = ctx.data_mut(|m| m.remove_temp::<Dlg>(id())).filter(|d| d.open) else { return };
    // the estimate and the check are about the scene as it is now
    let key = (app.scene.clone(), app.sel, d.opts.scope, d.opts.ignore_paint);
    let stale = d.snap.as_ref().is_none_or(|s| s.0 != key.0 || s.1 != key.1 || s.2 != key.2 || s.3 != key.3);
    if stale {
        d.snap = Some(key);
        d.check.reset();
        let (scene, sel, o) = (app.scene.clone(), app.sel, d.opts.clone());
        d.est.start(move |p| export_job::estimate(&scene, sel, &o, p));
    }
    if d.run_check {
        d.run_check = false;
        let (scene, sel, o) = (app.scene.clone(), app.sel, d.opts.clone());
        d.check.start(move |p| export_job::print_check(&scene, sel, &o, p));
    }
    if d.est.running() || d.check.running() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let busy = is_running(ctx);
    let mut close = false;
    let mut go = false;
    let mut check = false;
    egui::Area::new(egui::Id::new("export_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(440.0);
            ui.label(RichText::new("Export").strong().size(15.0));
            ui.label(RichText::new("Makes a file for 3D printers and other software").size(11.5).color(style::DIM));
            ui.add_space(4.0);
            let o = &mut d.opts;

            // Most people only need a purpose, a quality preset and the output scope.
            ui.label(RichText::new("① What is it for?").strong().size(12.5));
            ui.horizontal_wrapped(|ui| {
                for (format, label, hint) in [
                    (Format::Stl, "3D print", "STL: no colour. Works with most 3D printers"),
                    (Format::ThreeMf, "Colour print", "3MF: keeps colour and size"),
                    (Format::Glb, "Other apps", "GLB: for web and CG apps"),
                ] {
                    if style::pill(ui, label, o.format == format).on_hover_text(hint).clicked() {
                        choose_format(o, format);
                    }
                }
            });
            let explanation = match o.format {
                Format::Stl | Format::StlAscii => "STL is saved without colour",
                Format::ThreeMf => "3MF also keeps colour and size",
                Format::Glb => "GLB also keeps colour and material",
                _ => "Another file type is selected (you can change it in advanced settings)",
            };
            ui.label(RichText::new(explanation).size(11.5).color(style::DIM));
            ui.add_space(8.0);

            ui.label(RichText::new("② Finish detail").strong().size(12.5));
            ui.horizontal(|ui| {
                for (label, tol, hint) in [
                    ("Standard (recommended)", 0.2_f32, "Balance of quality and time"),
                    ("Fine", 0.1, "For fine shapes. Takes longer"),
                    ("Fast / light", 0.5, "For checking. Thin parts may disappear"),
                ] {
                    if style::pill(ui, label, (o.tolerance - tol).abs() < 1e-4).on_hover_text(hint).clicked() {
                        o.tolerance = tol;
                    }
                }
            });
            if quality_preset(o.tolerance).is_none() {
                ui.label(RichText::new(format!("Detail {:.3} mm (change in advanced settings)", o.tolerance)).size(11.5).color(style::KEEP));
            }
            ui.add_space(8.0);

            ui.label(RichText::new("③ What to export?").strong().size(12.5));
            ui.horizontal(|ui| {
                if style::pill(ui, "Visible shapes", o.scope == Scope::Visible).clicked() {
                    o.scope = Scope::Visible;
                }
                if style::pill_enabled(ui, "Selected only", o.scope == Scope::Selected, app.sel.is_some()).clicked() {
                    o.scope = Scope::Selected;
                }
                if style::pill(ui, "All", o.scope == Scope::All).on_hover_text("Includes hidden shapes").clicked() {
                    o.scope = Scope::All;
                }
            });
            if o.scope == Scope::Selected && app.sel.is_none() {
                ui.label(RichText::new("Select a shape first, or switch to 'Visible shapes'").size(11.5).color(style::KEEP));
            }
            ui.add_space(6.0);

            if let Some(result) = d.est.result() {
                match result {
                    Ok(e) => {
                        let tol = f64::from(o.tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE));
                        let raw = e.raw_tris(tol);
                        let secs = e.secs(tol, o.lighten != Lighten::Off);
                        if raw > (MAX_VERTS as f64) * 1.8 {
                            ui.label(RichText::new("Too fine. Try 'Fast / light'").size(11.5).color(style::CUT));
                        } else {
                            let col = if raw > 4.0e6 || secs > 60.0 { style::KEEP } else { style::DIM };
                            ui.label(
                                RichText::new(format!("About {} · max about {}", time_text(secs), bytes_text(raw * bytes_per_tri(o.format))))
                                    .size(11.5)
                                    .color(col),
                            );
                        }
                        ui.label(RichText::new(format!("Size: {}", size_text(e.size))).size(11.5).color(style::DIM));
                    }
                    Err(_) => {
                        ui.label(RichText::new("Couldn't check the size. Check the shape").size(11.5).color(style::KEEP));
                    }
                }
            } else if d.est.running() {
                ui.label(RichText::new("Checking size…").size(11.5).color(style::DIM));
            }

            ui.add_space(6.0);
            if style::pill(ui, if d.advanced { "Hide advanced ▲" } else { "Show advanced ▼" }, d.advanced).clicked() {
                d.advanced = !d.advanced;
            }
            if d.advanced {
                egui::ScrollArea::vertical().id_salt("ex_advanced_scroll").max_height(200.0).show(ui, |ui| {
                    // file type
                    row(ui, "File type", |ui| {
                        let items: Vec<(&str, &str)> = Format::ALL
                            .iter()
                            .map(|f| {
                                (
                                    f.label(),
                                    match f {
                                        Format::Stl => "For 3D printing (no colour, small)",
                                        Format::ThreeMf => "For slicers (colour and units)",
                                        Format::Obj => "Coloured (also makes an MTL file)",
                                        Format::Ply => "Coloured (vertex colour)",
                                        Format::Glb => "glTF 2.0 (for apps and web; colour and gloss)",
                                        Format::StlAscii => "Text STL (bigger file)",
                                    },
                                )
                            })
                            .collect();
                        let cur = Format::ALL.iter().position(|f| *f == o.format);
                        if let Some(i) = style::segmented(ui, "ex_format", &items, cur, false)
                            && let Some(f) = Format::ALL.get(i)
                        {
                            choose_format(o, *f);
                        }
                    });

                    // precision
                    row(ui, "Detail by number", |ui| {
                        for t in [0.05f32, 0.1, 0.2, 0.5] {
                            let on = (o.tolerance - t).abs() < 1e-4;
                            if style::pill(ui, &format!("{t}"), on).on_hover_text("Cell size (mm). Smaller is finer but heavier").clicked() {
                                o.tolerance = t;
                            }
                        }
                        style::field_sized(
                            ui,
                            [74.0, 22.0],
                            egui::DragValue::new(&mut o.tolerance).speed(0.005).range(MIN_TOLERANCE..=MAX_TOLERANCE).suffix(" mm").max_decimals(3),
                        );
                    });
                    row(ui, "Units", |ui| {
                        let items: Vec<(&str, &str)> =
                            Units::ALL.iter().map(|u| (u.label(), if o.format == Format::Glb { "GLB is always in m" } else { "" })).collect();
                        let cur = Units::ALL.iter().position(|u| *u == o.units);
                        if let Some(i) = style::segmented(ui, "ex_units", &items, cur, false)
                            && let Some(u) = Units::ALL.get(i)
                        {
                            o.units = *u;
                        }
                        ui.label(RichText::new("Size").color(style::DIM).size(12.0));
                        style::field_sized(
                            ui,
                            [68.0, 22.0],
                            egui::DragValue::new(&mut o.scale_pct).speed(1.0).range(1.0..=10000.0).suffix(" %").max_decimals(1),
                        );
                    });
                    row(ui, "Up direction", |ui| {
                        let items = [("Z (as is)", "3D printer style"), ("Y (OBJ / GLB style)", "Turns Z-up into Y-up")];
                        if let Some(i) = style::segmented(ui, "ex_up", &items, Some(usize::from(o.up == Up::Y)), false) {
                            o.up = if i == 1 { Up::Y } else { Up::Z };
                        }
                    });
                    row(ui, "Smooth", |ui| {
                        let items = [("None", ""), ("A little", ""), ("Normal", ""), ("Strong", "")];
                        if let Some(i) = style::segmented(ui, "ex_smooth", &items, Some(usize::from(o.smooth.min(3))), false) {
                            o.smooth = i as u8;
                        }
                        ui.label(RichText::new("Corners stay as they are").size(11.0).color(style::DIM));
                    });
                    row(ui, "Triangles", |ui| {
                        let items = [
                            ("As is", "All cells as triangles"),
                            ("Merge flat areas", "Flat faces into big triangles (shape stays the same)"),
                            ("Reduce count", "Set a triangle count to make it lighter"),
                        ];
                        let cur = match o.lighten {
                            Lighten::Off => 0,
                            Lighten::Flat => 1,
                            Lighten::Target(_) => 2,
                        };
                        if let Some(i) = style::segmented(ui, "ex_lighten", &items, Some(cur), false) {
                            o.lighten = match i {
                                0 => Lighten::Off,
                                1 => Lighten::Flat,
                                _ => Lighten::Target(match o.lighten {
                                    Lighten::Target(n) => n,
                                    _ => 100_000,
                                }),
                            };
                        }
                    });
                    if let Lighten::Target(n) = &mut o.lighten {
                        row(ui, "Guide", |ui| {
                            style::field_sized(ui, [110.0, 22.0], egui::DragValue::new(n).speed(500.0).range(100..=50_000_000).suffix(" pcs"));
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(o.format.has_colour(), |ui| {
                            style::switch(ui, &mut o.colour, "Include colour");
                        });
                        style::switch(ui, &mut o.ignore_paint, "Ignore painted colours");
                    });
                    if !o.format.has_colour() {
                        ui.label(RichText::new("STL can't include colour").size(11.0).color(style::DIM));
                    }

                    if let Some(Ok(e)) = d.est.result() {
                        let tris = e.raw_tris(f64::from(o.tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE)));
                        ui.label(RichText::new(format!("Triangles, about {}", count_text(tris))).size(11.5).color(style::DIM));
                    }
                });
            }

            // the print check
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if style::pill_enabled(ui, "Print check", false, !d.check.running())
                    .on_hover_text("size, volume, holes, thin spots, overhangs")
                    .clicked()
                {
                    check = true;
                }
                if d.check.running() {
                    ui.label(RichText::new("Checking…").size(11.5).color(style::DIM));
                }
            });
            match d.check.result() {
                Some(Ok(c)) => print_check_lines(ui, &c),
                Some(Err(e)) => {
                    ui.label(RichText::new(jp_error(&e)).size(11.5).color(style::CUT));
                }
                None => {}
            }

            if !d.msg.is_empty() {
                ui.label(RichText::new(&d.msg).size(11.5).color(style::CUT));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let b = egui::Button::new(RichText::new(format!("Export ({})…", o.format.label())).strong().color(Color32::WHITE))
                        .fill(style::ACCENT_SOFT)
                        .stroke(Stroke::new(1.0, style::ACCENT));
                    if ui.add(b).clicked() {
                        go = true;
                    }
                });
                if ui.button("Close").clicked() {
                    close = true;
                }
                if busy {
                    ui.label(RichText::new("Export is already running").size(11.5).color(style::DIM));
                }
            });
        });
    });
    d.opts = d.opts.sanitized_for_edit();
    if check {
        let (scene, sel, o) = (app.scene.clone(), app.sel, d.opts.clone());
        d.check.start(move |p| export_job::print_check(&scene, sel, &o, p));
    }
    if go {
        let o = d.opts.clone();
        close = start_with(app, ctx, o);
    }
    // a changed estimate setting starts a new estimate
    if !close {
        ctx.data_mut(|m| m.insert_temp(id(), d));
    }
}

fn print_check_lines(ui: &mut egui::Ui, c: &PrintCheck) {
    let small = |t: String, col: Color32| RichText::new(t).size(11.5).color(col);
    ui.label(small(format!("Size {}  volume {:.1} cm³", size_text(c.size), c.volume_cm3), style::DIM));
    if c.closed {
        ui.label(small("Surface: closed (OK to print)".into(), style::ADD));
    } else {
        ui.label(small("Surface: has holes (overlap shapes or make the blend stronger)".into(), style::CUT));
    }
    if c.thin_pct >= 0.5 {
        let t = c.thinnest.map(|t| format!(", thinnest spot about {t:.1} mm")).unwrap_or_default();
        ui.label(small(
            format!("Thin spots: about {:.0}% is thinner than {} mm{t} (may bend or fail to print)", c.thin_pct, export_job::THIN_MM),
            style::KEEP,
        ));
    } else {
        ui.label(small(format!("Thin spots: none (at least {} mm)", export_job::THIN_MM), style::ADD));
    }
    let col = if c.overhang_pct > 20.0 { style::KEEP } else { style::DIM };
    ui.label(small(
        format!(
            "Overhang: about {:.0}% of faces tilt more than 45° ({})",
            c.overhang_pct,
            if c.overhang_pct > 5.0 { "Might need supports" } else { "Not needed" }
        ),
        col,
    ));
}

impl Options {
    /// Keep the card's numbers in range while they are being edited (without moving a typed value).
    fn sanitized_for_edit(&self) -> Options {
        let mut o = self.clone();
        if !o.tolerance.is_finite() {
            o.tolerance = 0.2;
        }
        o.tolerance = o.tolerance.clamp(MIN_TOLERANCE, MAX_TOLERANCE);
        if !o.scale_pct.is_finite() {
            o.scale_pct = 100.0;
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut App, ctx: &egui::Context) {
        ctx.run_ui(egui::RawInput::default(), |ui| update(app, ui.ctx())).textures_delta.clear();
    }

    #[test]
    fn the_card_estimates_checks_and_exports_with_a_bar() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx, Some(ExportFormat::ThreeMf));
        frame(&mut app, &ctx);
        let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        assert_eq!(d.opts.format, Format::ThreeMf);
        // the estimate arrives from its thread
        let t0 = std::time::Instant::now();
        loop {
            frame(&mut app, &ctx);
            let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
            if let Some(r) = d.est.result() {
                let e = r.unwrap();
                assert!(e.area > 4000.0, "{}", e.area);
                break;
            }
            assert!(t0.elapsed().as_secs() < 30, "no estimate");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // the print check
        let mut d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
        d.run_check = true;
        ctx.data_mut(|m| m.insert_temp(id(), d));
        let t0 = std::time::Instant::now();
        loop {
            frame(&mut app, &ctx);
            let d = ctx.data(|m| m.get_temp::<Dlg>(id())).unwrap();
            if let Some(r) = d.check.result() {
                let c = r.unwrap();
                assert!(c.closed && (c.volume_cm3 - 33.5).abs() < 2.0, "{c:?}");
                break;
            }
            assert!(t0.elapsed().as_secs() < 60, "no check");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // an export runs in the background and ends with a message and a file
        let dir = std::env::temp_dir().join(format!("yura_dlg_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.3mf");
        let o = Options { tolerance: 0.5, format: Format::ThreeMf, ..Options::default() };
        assert!(start_job(&mut app, &ctx, o.clone(), path.clone()));
        assert!(is_running(&ctx));
        assert!(!start_job(&mut app, &ctx, o, dir.join("b.3mf")), "one at a time");
        let t0 = std::time::Instant::now();
        while is_running(&ctx) {
            frame(&mut app, &ctx);
            assert!(t0.elapsed().as_secs() < 60, "export did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(path.exists());
        assert!(last(&ctx).is_some_and(|o| o.format == Format::ThreeMf));
        let _ = std::fs::remove_dir_all(&dir);
        // an error is said in Japanese
        assert_eq!(jp_error(CANCELLED), "Cancelled");
        assert!(jp_error("the model is too big for this precision; choose a coarser one").contains("Detail"));
    }

    #[test]
    fn simple_presets_keep_saved_scope_scale_and_axis() {
        assert!(!has_custom_settings(&Options::default()));
        let mut opts = Options { units: Units::Cm, scale_pct: 142.0, scope: Scope::Selected, ..Options::default() };
        assert!(has_custom_settings(&opts));
        choose_format(&mut opts, Format::Glb);
        assert_eq!(opts.up, Up::Y);
        assert_eq!(opts.units, Units::Cm);
        assert_eq!(opts.scale_pct, 142.0);
        assert_eq!(opts.scope, Scope::Selected);
        choose_format(&mut opts, Format::ThreeMf);
        assert_eq!(opts.up, Up::Z);
        assert_eq!(quality_preset(0.2), Some(0));
        assert_eq!(quality_preset(0.1), Some(1));
        assert_eq!(quality_preset(0.5), Some(2));
        assert_eq!(quality_preset(0.15), None);
    }

    #[test]
    fn saved_custom_options_open_with_details_visible() {
        let ctx = egui::Context::default();
        let mut o = Options::default();
        o.scale_pct = 125.0;
        ctx.data_mut(|m| m.insert_temp(last_id(), o));
        let d = new_dlg(&ctx, None);
        assert!(d.advanced);
        assert_eq!(d.opts.scale_pct, 125.0);
        assert!(!new_dlg(&egui::Context::default(), None).advanced);
    }

    #[test]
    fn words_for_numbers_read_well() {
        assert_eq!(count_text(1234.0), "1234");
        assert_eq!(count_text(2_500_000.0), "250 x10K");
        assert_eq!(count_text(3.0e8), "3.0 x100M");
        assert_eq!(time_text(0.2), "about 1 sec");
        assert_eq!(time_text(30.0), "about 30 sec");
        assert_eq!(time_text(600.0), "about 10 min");
        assert_eq!(bytes_text(3.0e6), "3MB");
    }
}
