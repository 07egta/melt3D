//! Look and feel: soft neutral greys with one violet accent, rounded and quiet.

use egui::{Color32, CornerRadius, Painter, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const PANEL: Color32 = Color32::from_rgb(38, 38, 42);
pub const PANEL_DARK: Color32 = Color32::from_rgb(30, 30, 34);
pub const CARD: Color32 = Color32::from_rgb(48, 48, 54);
pub const HOVER: Color32 = Color32::from_rgb(62, 62, 70);
pub const BORDER: Color32 = Color32::from_rgb(64, 64, 72);
pub const TEXT: Color32 = Color32::from_rgb(222, 222, 228);
pub const DIM: Color32 = Color32::from_rgb(140, 140, 152);
pub const ACCENT: Color32 = Color32::from_rgb(146, 126, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(70, 62, 120);
pub const ADD: Color32 = Color32::from_rgb(120, 214, 150);
pub const CUT: Color32 = Color32::from_rgb(244, 124, 132);
pub const AXIS: [Color32; 3] = [Color32::from_rgb(232, 84, 84), Color32::from_rgb(110, 200, 100), Color32::from_rgb(80, 140, 240)];

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL_DARK;
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    let round = CornerRadius::same(7);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = round;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_SOFT);
    v.widgets.active.bg_fill = ACCENT_SOFT;
    v.widgets.active.weak_bg_fill = ACCENT_SOFT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.slider_width = 120.0;
    });
}

/// A square button drawn by `paint` (an icon from `glyphs`). `on` shows it as the chosen one.
pub fn icon_button(ui: &mut Ui, size: f32, tip: &str, on: bool, paint: impl FnOnce(&Painter, Rect, Color32)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 8.0, ACCENT_SOFT);
        p.rect_stroke(rect, 8.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, HOVER);
    }
    let ink = if on || resp.hovered() { Color32::WHITE } else { TEXT };
    paint(p, rect.shrink(size * 0.08), ink);
    resp.on_hover_text(tip)
}

/// A pill button with text; `on` highlights it.
pub fn pill(ui: &mut Ui, text: &str, on: bool) -> Response {
    let b = egui::Button::new(egui::RichText::new(text).size(12.5).color(if on { Color32::WHITE } else { TEXT }))
        .fill(if on { ACCENT_SOFT } else { CARD })
        .stroke(if on { Stroke::new(1.0, ACCENT) } else { Stroke::NONE })
        .corner_radius(CornerRadius::same(9));
    ui.add(b)
}

/// A section title.
pub fn title(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).size(12.0).strong().color(DIM));
}

/// Three numbers on a row, X Y Z coloured like the axes. Returns whether any changed.
pub fn xyz(ui: &mut Ui, label: &str, v: &mut [f32; 3], speed: f64, range: std::ops::RangeInclusive<f32>, suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([44.0, 18.0], egui::Label::new(egui::RichText::new(label).color(DIM).size(12.0)));
        for (i, c) in v.iter_mut().enumerate() {
            ui.label(egui::RichText::new(["X", "Y", "Z"][i]).color(AXIS[i]).size(11.0).strong());
            changed |= ui.add(egui::DragValue::new(c).speed(speed).range(range.clone()).suffix(suffix).max_decimals(2)).changed();
        }
    });
    changed
}
