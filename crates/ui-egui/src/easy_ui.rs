//! The easy layout: Plasticity-style chrome around the viewport.
//!
//! * a tool strip down the left of the viewport (modelling tools; sketch tools while sketching),
//! * selection chips at its top left (what a click picks: body, face, edge, vertex),
//!   with switches for the settings panel and the ribbon,
//! * a settings panel on the right: plane buttons, display, camera.
//!
//! Everything here calls the same commands as the ribbon and the search (F, or S): nothing is
//! new to the engine. Press F anywhere to search every command.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;
use solidcraft_engine::CommandSpec;

use crate::SolveApp;
use crate::i18n::tr;
use crate::icons;
use crate::theme::Tokens;

/// Size of a strip button, and of the gap a divider takes.
const BTN: f32 = 34.0;
const DIVIDER: f32 = 9.0;

/// Width of the settings panel.
pub const PANEL_WIDTH: f32 = 236.0;

/// The strip while modelling; `None` is a divider. Commands not built yet are left out.
const MODEL_TOOLS: &[Option<&str>] = &[
    Some("sketch.line"),
    Some("sketch.spline.control_point"),
    Some("sketch.circle.center"),
    Some("sketch.rectangle.two_point"),
    None,
    Some("solid.extrude"),
    Some("solid.revolve"),
    Some("solid.sweep"),
    Some("solid.loft"),
    None,
    Some("solid.box"),
    Some("solid.cylinder"),
    Some("solid.sphere"),
    None,
    Some("solid.fillet"),
    Some("solid.chamfer"),
    Some("solid.shell"),
    Some("solid.combine"),
    Some("solid.melt"),
    None,
    Some("surface.patch"),
    Some("surface.blend"),
    Some("solid.move"),
    Some("solid.pattern.rectangular"),
];

/// The strip while a sketch is open.
const SKETCH_TOOLS: &[Option<&str>] = &[
    Some("sketch.line"),
    Some("sketch.rectangle.two_point"),
    Some("sketch.circle.center"),
    Some("sketch.arc.three_point"),
    Some("sketch.polygon.inscribed"),
    Some("sketch.spline.fit_point"),
    Some("sketch.spline.control_point"),
    None,
    Some("sketch.trim"),
    Some("sketch.offset"),
    Some("sketch.fillet"),
    Some("sketch.mirror"),
    Some("sketch.dimension"),
    None,
    Some("sketch.finish"),
];

/// What a click picks, as the chips show it: label id, selection filter kind.
const PICK_CHIPS: &[(&str, &str)] =
    &[("easy.pick_body", "body"), ("easy.pick_face", "faces"), ("easy.pick_edge", "edges"), ("easy.pick_vertex", "vertices")];

/// The overlays of the easy layout, drawn over the viewport (`rect`).
pub fn overlays(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    if !app.ui.easy_layout {
        return;
    }
    top_left(app, ui, rect);
    tool_strip(app, ui, rect);
}

/// Whether a click picks things of this kind now.
fn picks(app: &SolveApp, kind: &str) -> bool {
    if kind == "body" { app.ui.pick_bodies } else { !app.ui.pick_off.iter().any(|k| k == kind) }
}

fn toggle_pick(app: &mut SolveApp, kind: &str) {
    if kind == "body" {
        app.ui.pick_bodies = !app.ui.pick_bodies;
    } else if let Some(i) = app.ui.pick_off.iter().position(|k| k == kind) {
        app.ui.pick_off.remove(i);
    } else {
        app.ui.pick_off.push(kind.to_string());
    }
}

/// The selection chips, then the switches for the settings panel and the ribbon.
fn top_left(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get();
    let painter = ui.painter_at(rect);
    let chip_w = 52.0;
    let h = 28.0;
    let switches = 2.0 * (h + 4.0);
    let width = PICK_CHIPS.len() as f32 * (chip_w + 3.0) + 10.0 + 8.0 + switches;
    let bar = Rect::from_min_size(pos2(rect.left() + 10.0, rect.top() + 10.0), vec2(width, h + 8.0));
    // The bar takes clicks that land between its buttons.
    ui.interact(bar, ui.id().with("easy_top_bar"), Sense::click_and_drag());
    painter.rect(bar, 8.0, t.overlay, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let mut x = bar.left() + 5.0;
    let mut clicked: Option<&str> = None;
    for (label, kind) in PICK_CHIPS {
        let r = Rect::from_min_size(pos2(x, bar.top() + 4.0), vec2(chip_w, h));
        let on = picks(app, kind);
        let resp = ui.interact(r, ui.id().with(("easy_pick", *kind)), Sense::click());
        crate::scenario::publish_handle(&format!("easy:pick:{kind}"), r.center());
        let fill = if on {
            t.accent
        } else if resp.hovered() {
            t.hover
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(r, 6.0, fill);
        let text = if on { t.text } else { t.text_dim };
        painter.text(r.center(), Align2::CENTER_CENTER, tr(label), FontId::proportional(12.0), text);
        if resp.on_hover_text(tr("easy.pick_tip")).clicked() {
            clicked = Some(kind);
        }
        x += chip_w + 3.0;
    }
    if let Some(kind) = clicked {
        toggle_pick(app, kind);
    }
    x += 8.0;
    painter.vline(x - 6.0, (bar.top() + 8.0)..=(bar.bottom() - 8.0), Stroke::new(1.0, t.border));
    for (i, name) in ["panel", "ribbon"].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(x + i as f32 * (h + 4.0), bar.top() + 4.0), vec2(h, h));
        let on = if name == "panel" { app.ui.show_inspector } else { app.ui.show_ribbon };
        let resp = ui.interact(r, ui.id().with(("easy_switch", name)), Sense::click());
        crate::scenario::publish_handle(&format!("easy:switch:{name}"), r.center());
        if on || resp.hovered() {
            painter.rect_filled(r, 6.0, if on { t.accent_soft } else { t.hover });
        }
        panel_glyph(&painter, r.shrink(7.0), name == "panel", if on { t.accent } else { t.icon });
        let tip = tr(if name == "panel" { "easy.inspector" } else { "easy.ribbon" });
        if resp.on_hover_text(tip).clicked() {
            if name == "panel" {
                app.ui.show_inspector = !app.ui.show_inspector;
            } else {
                app.ui.show_ribbon = !app.ui.show_ribbon;
            }
        }
    }
}

/// A small window icon: a frame with the right side (settings panel) or the top (ribbon) filled.
fn panel_glyph(painter: &egui::Painter, r: Rect, side: bool, color: Color32) {
    painter.rect_stroke(r, 2.0, Stroke::new(1.4, color), egui::StrokeKind::Inside);
    let fill = if side {
        Rect::from_min_max(pos2(r.right() - r.width() * 0.38, r.top()), r.right_bottom())
    } else {
        Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + r.height() * 0.38))
    };
    painter.rect_filled(fill, 1.0, color);
}

/// The tools of the strip that are built, with their dividers.
fn strip_items(sketching: bool) -> Vec<Option<&'static CommandSpec>> {
    let list = if sketching { SKETCH_TOOLS } else { MODEL_TOOLS };
    list.iter()
        .filter_map(|item| match item {
            None => Some(None),
            Some(id) => solidcraft_engine::find_command(id).filter(|c| c.planned.is_none()).map(Some),
        })
        .collect()
}

/// The vertical tool strip. What does not fit the window's height goes behind a "more" button
/// that opens the command search.
fn tool_strip(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    let t = Tokens::get();
    let sketching = app.session.active_sketch.is_some();
    let items = strip_items(sketching);
    let top = rect.top() + 56.0;
    // Room for the strip: below the chips, above the navigation bar.
    let room = (rect.height() - 56.0 - 64.0).max(BTN * 3.0);
    let more_h = BTN + 2.0;
    let mut used = 8.0;
    let mut shown: Vec<Option<&'static CommandSpec>> = Vec::new();
    let mut truncated = false;
    for item in &items {
        let need = if item.is_some() { BTN + 2.0 } else { DIVIDER };
        if used + need + more_h > room && shown.len() + 1 < items.len() {
            truncated = true;
            break;
        }
        shown.push(*item);
        used += need;
    }
    // A divider at either end is noise.
    while matches!(shown.last(), Some(None)) {
        shown.pop();
        used -= DIVIDER;
    }
    let height = used + if truncated { more_h } else { 0.0 } + 4.0;
    let bar = Rect::from_min_size(pos2(rect.left() + 10.0, top), vec2(BTN + 10.0, height));
    crate::scenario::publish_rect("easy:strip", bar);
    ui.interact(bar, ui.id().with("easy_strip_bar"), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect(bar, 8.0, t.overlay, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let mut y = bar.top() + 4.0;
    let mut clicked: Option<&'static str> = None;
    let mut more = false;
    for item in shown {
        match item {
            None => {
                painter.hline((bar.left() + 8.0)..=(bar.right() - 8.0), y + DIVIDER / 2.0 - 1.0, Stroke::new(1.0, t.border));
                y += DIVIDER;
            }
            Some(c) => {
                let br = Rect::from_min_size(pos2(bar.left() + 5.0, y), vec2(BTN, BTN));
                // A drawing tool opens its own sketch, so it is never greyed out for want of one.
                let enabled = c.info(&app.session).enabled || crate::easy::auto_sketch(c.id);
                let resp = ui.interact(br, ui.id().with(("easy_tool", c.id)), Sense::click());
                crate::scenario::publish_handle(&format!("easy:tool:{}", c.id), br.center());
                if enabled && resp.hovered() {
                    painter.rect_filled(br, 6.0, t.hover);
                }
                let dim = if enabled { 1.0 } else { 0.35 };
                icons::paint(
                    &painter,
                    br.shrink(6.0),
                    c.icon,
                    t.icon.gamma_multiply(dim),
                    t.icon_fill.gamma_multiply(dim),
                    t.accent.gamma_multiply(dim),
                );
                let tip = format!("{}{}", crate::i18n::tr_label(c.label), c.shortcut.map(|k| format!("  ({k})")).unwrap_or_default());
                if resp.on_hover_text(tip).clicked() && enabled {
                    clicked = Some(c.id);
                }
                y += BTN + 2.0;
            }
        }
    }
    if truncated {
        let br = Rect::from_min_size(pos2(bar.left() + 5.0, y), vec2(BTN, BTN));
        let resp = ui.interact(br, ui.id().with("easy_more"), Sense::click());
        crate::scenario::publish_handle("easy:more", br.center());
        if resp.hovered() {
            painter.rect_filled(br, 6.0, t.hover);
        }
        painter.text(br.center(), Align2::CENTER_CENTER, "…", FontId::proportional(18.0), t.text);
        more = resp.on_hover_text(tr("easy.more")).clicked();
    }
    if let Some(id) = clicked {
        app.start(id);
    } else if more {
        crate::shortcut_box::open(app, None);
    }
}

/// The settings panel on the right of the window.
pub fn inspector(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    egui::Panel::right("sc_inspector")
        .exact_size(PANEL_WIDTH)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(12, 8)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                plane_section(app, ui);
                display_section(app, ui);
                camera_section(app, ui);
            });
        });
}

fn header(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get();
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong().size(12.5).color(t.text));
    ui.add_space(3.0);
}

/// A label with an on/off switch at its right.
fn switch(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    let t = Tokens::get();
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.5).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
            if resp.clicked() {
                *on = !*on;
            }
            let fill = if *on { t.accent } else { t.border };
            ui.painter().rect_filled(rect, 10.0, fill);
            let cx = if *on { rect.right() - 10.0 } else { rect.left() + 10.0 };
            ui.painter().circle_filled(pos2(cx, rect.center().y), 7.0, t.text);
        });
    });
}

/// Plane buttons: draw on XY, YZ or XZ straight away, or make a construction plane.
fn plane_section(app: &mut SolveApp, ui: &mut egui::Ui) {
    header(ui, &tr("easy.plane"));
    let gap = ui.spacing().item_spacing.x;
    let w = (ui.available_width() - 2.0 * gap) / 3.0;
    ui.horizontal(|ui| {
        for plane in ["XY", "YZ", "XZ"] {
            let r = ui.add_sized([w, 30.0], egui::Button::new(RichText::new(plane).strong()));
            crate::scenario::publish_handle(&format!("easy:plane:{plane}"), r.rect.center());
            if r.on_hover_text(tr("easy.plane_hint")).clicked() {
                let _ = app.run("sketch.create", json!({"plane": plane}));
            }
        }
    });
    ui.add_space(4.0);
    let make = ui.add_sized([ui.available_width(), 28.0], egui::Button::new(tr("easy.make_plane")));
    let id = ui.id().with("easy_make_plane");
    egui::Popup::menu(&make).id(id).show(|ui| {
        for cid in [
            "construct.plane.offset",
            "construct.plane.three_points",
            "construct.plane.midplane",
            "construct.plane.at_angle",
            "construct.plane.tangent",
            "construct.plane.perpendicular",
            "construct.plane.along_path",
            "construct.plane.two_edges",
        ] {
            if let Some(c) = solidcraft_engine::find_command(cid) {
                // A drawing tool opens its own sketch, so it is never greyed out for want of one.
                let enabled = c.info(&app.session).enabled || crate::easy::auto_sketch(c.id);
                if ui.add_enabled(enabled, egui::Button::new(crate::i18n::tr_label(c.label)).frame(false)).clicked() {
                    app.start(cid);
                    ui.close();
                }
            }
        }
    });
}

fn display_section(app: &mut SolveApp, ui: &mut egui::Ui) {
    header(ui, &tr("easy.display"));
    ui.horizontal_wrapped(|ui| {
        for (i, key) in ["easy.style_shaded_edges", "easy.style_shaded", "easy.style_wire", "easy.style_hidden"].into_iter().enumerate() {
            if ui.selectable_label(usize::from(app.ui.visual_style) == i, tr(key)).clicked() {
                app.ui.visual_style = i as u8;
            }
        }
    });
    ui.add_space(4.0);
    switch(ui, &tr("easy.grid"), &mut app.ui.show_grid);
    switch(ui, &tr("easy.origin"), &mut app.ui.show_origin);
    switch(ui, &tr("easy.ground_shadow"), &mut app.ui.ground_shadow);
    switch(ui, &tr("easy.sketches"), &mut app.ui.show_sketches);
}

fn camera_section(app: &mut SolveApp, ui: &mut egui::Ui) {
    header(ui, &tr("easy.camera"));
    switch(ui, &tr("easy.perspective"), &mut app.ui.perspective);
    ui.add_space(4.0);
    if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(tr("easy.reset_camera"))).clicked() {
        app.animate_view("home");
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn easy() -> crate::scenario::Harness {
        let mut h = crate::scenario::Harness::new();
        h.app.enable_easy_layout();
        h.frames(3);
        h
    }

    #[test]
    fn the_strip_lists_the_core_tools_and_hides_what_is_not_built() {
        let ids: Vec<&str> = super::strip_items(false).into_iter().flatten().map(|c| c.id).collect();
        assert!(["solid.extrude", "solid.sweep", "solid.loft", "solid.fillet", "solid.combine"].iter().all(|id| ids.contains(id)), "{ids:?}");
        let sketch: Vec<&str> = super::strip_items(true).into_iter().flatten().map(|c| c.id).collect();
        assert!(sketch.contains(&"sketch.line") && sketch.contains(&"sketch.finish"), "{sketch:?}");
    }

    #[test]
    fn a_click_on_line_draws_straight_away_on_the_xy_plane() {
        let mut h = easy();
        assert!(h.app.session.active_sketch.is_none());
        h.step(&json!({"click": {"handle": "easy:tool:sketch.line"}})).unwrap();
        h.frames(3);
        assert!(h.app.session.active_sketch.is_some(), "a sketch was opened for the line: {:?}", h.app.status);
        assert_eq!(h.app.ui.tab, "SKETCH");
    }

    #[test]
    fn the_chips_switch_what_a_click_picks() {
        let mut h = easy();
        assert!(h.app.ui.pick_off.is_empty());
        h.step(&json!({"click": {"handle": "easy:pick:faces"}})).unwrap();
        assert!(h.app.ui.pick_off.iter().any(|k| k == "faces"));
        h.step(&json!({"click": {"handle": "easy:pick:faces"}})).unwrap();
        assert!(h.app.ui.pick_off.is_empty());
        h.step(&json!({"click": {"handle": "easy:pick:body"}})).unwrap();
        assert!(h.app.ui.pick_bodies);
    }

    #[test]
    fn the_plane_buttons_start_a_sketch_and_the_switches_hide_the_panels() {
        let mut h = easy();
        h.step(&json!({"click": {"handle": "easy:plane:XZ"}})).unwrap();
        assert!(h.app.session.active_sketch.is_some());
        assert!(h.app.ui.show_inspector);
        h.step(&json!({"click": {"handle": "easy:switch:panel"}})).unwrap();
        assert!(!h.app.ui.show_inspector);
    }
}
