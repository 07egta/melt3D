//! Look and feel: a blue-violet dark theme with one violet accent. Surfaces are soft and raised:
//! gradients, a bright lip on top, a dark underside, layered shadows. Hover lifts, press sinks,
//! chosen things glow. Every animation is at most 120 ms and only asks for repaints while moving.

use egui::epaint::Shadow;
use egui::{
    Align2, Color32, CornerRadius, FontId, Margin, Painter, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, vec2,
};

#[path = "style_gfx.rs"]
pub mod gfx;

#[cfg(test)]
#[path = "style_preview.rs"]
pub(crate) mod preview;

use gfx::{Buf, Sink, lerp, lit, mix};

// ----- colours -----

/// Bars (top, bottom) and the deepest wells.
pub const PANEL_DARK: Color32 = Color32::from_rgb(22, 23, 31);
/// Side panel.
pub const PANEL: Color32 = Color32::from_rgb(31, 32, 43);
/// Raised plates and buttons at rest.
pub const CARD: Color32 = Color32::from_rgb(45, 47, 63);
pub const HOVER: Color32 = Color32::from_rgb(60, 63, 85);
#[allow(dead_code)]
pub const BORDER: Color32 = Color32::from_rgb(66, 69, 92);
pub const TEXT: Color32 = Color32::from_rgb(232, 233, 242);
/// Labels (contrast 5:1 or more on the panels).
pub const DIM: Color32 = Color32::from_rgb(152, 156, 178);
pub const ACCENT: Color32 = Color32::from_rgb(150, 128, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(74, 64, 140);
pub const ADD: Color32 = Color32::from_rgb(120, 214, 150);
pub const CUT: Color32 = Color32::from_rgb(244, 124, 132);
pub const PAINT: Color32 = Color32::from_rgb(120, 190, 255);
pub const AXIS: [Color32; 3] = [Color32::from_rgb(232, 84, 84), Color32::from_rgb(110, 200, 100), Color32::from_rgb(80, 140, 240)];

/// Sunken wells (text fields, slider rails, segmented tracks).
pub const INSET_TOP: Color32 = Color32::from_rgb(15, 16, 22);
pub const INSET_BOTTOM: Color32 = Color32::from_rgb(25, 26, 36);

const BTN_TOP: Color32 = Color32::from_rgb(68, 71, 96);
const BTN_BOTTOM: Color32 = Color32::from_rgb(43, 45, 62);
const BTN_EDGE: Color32 = Color32::from_rgb(12, 12, 19);
const BTN_HOT_TOP: Color32 = Color32::from_rgb(88, 92, 124);
const BTN_HOT_BOTTOM: Color32 = Color32::from_rgb(54, 57, 80);
const BTN_ON_TOP: Color32 = Color32::from_rgb(128, 108, 244);
const BTN_ON_BOTTOM: Color32 = Color32::from_rgb(78, 63, 172);
const BTN_ON_EDGE: Color32 = Color32::from_rgb(190, 176, 255);
const BTN_DOWN_TOP: Color32 = Color32::from_rgb(27, 28, 39);
const BTN_DOWN_BOTTOM: Color32 = Color32::from_rgb(41, 43, 60);

/// Fastest animation we use (seconds).
const FAST: f32 = 0.10;
const MEDIUM: f32 = 0.12;

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(36, 38, 52);
    v.extreme_bg_color = INSET_TOP;
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(26));
    v.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(130) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(120) };
    let round = CornerRadius::same(8);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = round;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(20));
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(22));
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(ACCENT_SOFT, ACCENT, 0.5));
    v.widgets.active.bg_fill = ACCENT_SOFT;
    v.widgets.active.weak_bg_fill = ACCENT_SOFT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.open.bg_fill = HOVER;
    v.widgets.open.weak_bg_fill = HOVER;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.slider_width = 120.0;
        s.spacing.interact_size = vec2(40.0, 24.0);
        s.spacing.menu_margin = Margin::same(6);
        // nothing smaller than 12 px
        for (style, size) in
            [(TextStyle::Small, 12.0), (TextStyle::Body, 13.5), (TextStyle::Button, 13.5), (TextStyle::Monospace, 13.0), (TextStyle::Heading, 18.0)]
        {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
    });
}

// ----- the raised button surface -----

/// Top colour, bottom colour and edge colour of a button, from how hovered (`hot`) and chosen (`on`)
/// it is (both 0..1) and whether it is held down.
pub fn button_look(hot: f32, on: f32, pressed: bool) -> (Color32, Color32, Color32) {
    if pressed {
        let on_k = mix(BTN_DOWN_TOP, lit(BTN_ON_BOTTOM, 0.8), on);
        return (on_k, mix(BTN_DOWN_BOTTOM, BTN_ON_BOTTOM, on), mix(BTN_EDGE, BTN_ON_EDGE, on * 0.7));
    }
    let top = mix(mix(BTN_TOP, BTN_HOT_TOP, hot), BTN_ON_TOP, on);
    let bottom = mix(mix(BTN_BOTTOM, BTN_HOT_BOTTOM, hot), BTN_ON_BOTTOM, on);
    let edge = mix(mix(BTN_EDGE, Color32::from_rgb(70, 74, 104), hot * 0.6), BTN_ON_EDGE, on);
    (top, bottom, edge)
}

/// Draws a raised, lit button surface into `out`. `hot` and `on` are 0..1 animation values.
fn raised(out: &impl Sink, rect: Rect, radius: f32, hot: f32, on: f32, pressed: bool) {
    if !pressed {
        gfx::shadow(out, rect, radius, &[(1, 2, 0, 80), (3, (5.0 + 4.0 * hot) as u8, 0, (50.0 * hot) as u8)]);
    }
    if on > 0.01 {
        gfx::glow(out, rect, radius, Color32::from_rgba_unmultiplied(150, 128, 255, (95.0 * on) as u8), 12);
    }
    let (top, bottom, edge) = button_look(hot, on, pressed);
    gfx::vgrad(out, rect, radius, top, bottom);
    out.put(Shape::rect_stroke(rect, radius, Stroke::new(1.0, edge), StrokeKind::Inside));
    if pressed {
        // a dark lip: the surface is pushed in
        gfx::top_lip(out, rect, radius, Color32::from_black_alpha(90));
    } else {
        gfx::top_lip(out, rect, radius, Color32::from_white_alpha((48.0 + 52.0 * hot + 34.0 * on) as u8));
        gfx::bottom_shade(out, rect, radius, Color32::from_black_alpha(50));
    }
}

/// Paints the surface for a clickable thing and returns how far the content should move
/// (up when hovered, down when held).
pub fn paint_button(ui: &Ui, resp: &Response, rect: Rect, radius: f32, on: bool) -> Vec2 {
    let enabled = ui.is_enabled();
    let ctx = ui.ctx();
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() && enabled { 1.0 } else { 0.0 }, FAST);
    let on_t = ctx.animate_value_with_time(resp.id.with("on"), if on { 1.0 } else { 0.0 }, MEDIUM);
    let pressed = enabled && resp.is_pointer_button_down_on();
    let shift = if pressed { vec2(0.0, 1.0) } else { vec2(0.0, -hot) };
    raised(ui.painter(), rect.translate(shift), radius, hot, on_t, pressed);
    shift
}

/// A square button drawn by `paint` (an icon from `glyphs`). `on` shows it as the chosen one.
pub fn icon_button(ui: &mut Ui, size: f32, tip: &str, on: bool, paint: impl FnOnce(&Painter, Rect, Color32)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let shift = paint_button(ui, &resp, rect, (size * 0.3).clamp(7.0, 11.0), on);
    let hot = ui.ctx().animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let ink = if on { Color32::WHITE } else { mix(TEXT, Color32::WHITE, hot) };
    paint(ui.painter(), rect.shrink(size * 0.06).translate(shift), ink);
    resp.on_hover_text(tip)
}

/// A button with text. `on` highlights it as the chosen one.
pub fn pill(ui: &mut Ui, text: &str, on: bool) -> Response {
    text_button(ui, text, on, true, None)
}

/// Like [`pill`], greyed out and not clickable when `enabled` is false.
pub fn pill_enabled(ui: &mut Ui, text: &str, on: bool, enabled: bool) -> Response {
    text_button(ui, text, on, enabled, None)
}

fn text_button(ui: &mut Ui, text: &str, on: bool, enabled: bool, color: Option<Color32>) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(13.5), Color32::PLACEHOLDER);
    let size = vec2((galley.size().x + 24.0).max(30.0), 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let shift = paint_button(ui, &resp, rect, 9.0, on);
    let hot = ui.ctx().animate_value_with_time(resp.id.with("hot"), if resp.hovered() && enabled { 1.0 } else { 0.0 }, FAST);
    let base = color.unwrap_or(TEXT);
    let col = if !enabled {
        mix(base, BTN_BOTTOM, 0.62)
    } else if on {
        Color32::WHITE
    } else {
        mix(base, Color32::WHITE, hot * 0.6)
    };
    let pos = rect.center() - galley.size() * 0.5 + shift;
    ui.painter().galley(pos, galley, col);
    resp
}

/// A menu button that looks like the other buttons. `accent` colours its text; the menu opens below it.
pub fn menu_button<R>(ui: &mut Ui, text: &str, accent: bool, add_contents: impl FnOnce(&mut Ui) -> R) -> Option<egui::InnerResponse<R>> {
    use egui::containers::menu::MenuConfig;
    let resp = text_button(ui, text, false, true, if accent { Some(ACCENT) } else { None });
    let config = MenuConfig::find(ui);
    egui::Popup::menu(&resp)
        .close_behavior(config.close_behavior)
        .style(config.style.clone())
        .info(egui::UiStackInfo::new(egui::UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, config))
        .show(add_contents)
}

/// Slides a raised highlight between the items; returns the index clicked. `items` are `(label, tip)`.
/// `stretch` makes the control fill the width that is left.
pub fn segmented(ui: &mut Ui, salt: &str, items: &[(&str, &str)], selected: Option<usize>, stretch: bool) -> Option<usize> {
    let font = FontId::proportional(13.5);
    let galleys: Vec<_> = items.iter().map(|(l, _)| ui.painter().layout_no_wrap((*l).to_string(), font.clone(), Color32::PLACEHOLDER)).collect();
    let (h, gap) = (34.0, 3.0);
    let text_sum: f32 = galleys.iter().map(|g| g.size().x).sum();
    let n = galleys.len().max(1) as f32;
    // roomy by default; when stretching into a narrow space the padding shrinks so nothing spills out
    let mut pad = 14.0;
    if stretch {
        pad = ((ui.available_width() - gap * 2.0 - text_sum) / (2.0 * n)).clamp(3.0, 14.0);
    }
    let mut widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad * 2.0).collect();
    if stretch && !widths.is_empty() {
        let total: f32 = widths.iter().sum();
        let avail = ui.available_width() - gap * 2.0;
        if avail > total {
            let extra = (avail - total) / widths.len() as f32;
            widths.iter_mut().for_each(|w| *w += extra);
        }
    }
    let total = widths.iter().sum::<f32>() + gap * 2.0;
    let (track, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let id = ui.make_persistent_id(salt);
    let ctx = ui.ctx().clone();
    // the sunken track
    let p = ui.painter();
    gfx::vgrad(p, track, 12.0, INSET_TOP, INSET_BOTTOM);
    p.rect_stroke(track, 12.0, Stroke::new(1.0, Color32::from_white_alpha(16)), StrokeKind::Inside);
    gfx::top_lip(p, track, 12.0, Color32::from_black_alpha(110));
    // item rectangles
    let mut x = track.left() + gap;
    let rects: Vec<Rect> = widths
        .iter()
        .map(|w| {
            let r = Rect::from_min_size(pos2(x, track.top() + gap), vec2(*w, h - gap * 2.0));
            x += w;
            r
        })
        .collect();
    // the sliding highlight
    if let Some(target) = selected.and_then(|i| rects.get(i).copied()) {
        let vis = ctx.animate_value_with_time(id.with("vis"), 1.0, MEDIUM);
        let hx = ctx.animate_value_with_time(id.with("x"), target.left() - track.left(), MEDIUM);
        let hw = ctx.animate_value_with_time(id.with("w"), target.width(), MEDIUM);
        let r = Rect::from_min_size(pos2(track.left() + hx, target.top()), vec2(hw, target.height()));
        raised(p, r, 9.0, 0.0, vis, false);
    } else {
        ctx.animate_value_with_time(id.with("vis"), 0.0, MEDIUM);
    }
    let mut clicked = None;
    for (i, ((label_galley, rect), (_, tip))) in galleys.into_iter().zip(&rects).zip(items).enumerate() {
        let resp = ui.interact(*rect, id.with(("item", i)), Sense::click());
        let hot = ctx.animate_value_with_time(id.with(("hot", i)), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
        let col = if selected == Some(i) { Color32::WHITE } else { mix(DIM, TEXT, 0.35 + 0.65 * hot) };
        let pos = rect.center() - label_galley.size() * 0.5;
        ui.painter().galley(pos, label_galley, col);
        if resp.clicked() {
            clicked = Some(i);
        }
        if !tip.is_empty() {
            resp.on_hover_text(*tip);
        }
    }
    clicked
}

// ----- text and small marks -----

/// A section heading: an accent tick, small caps text and a hairline running to the edge.
pub fn title(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), FontId::proportional(12.0), Color32::PLACEHOLDER);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let p = ui.painter();
    let c = rect.center().y;
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), c - 6.0), pos2(rect.left() + 3.0, c + 6.0)), 1.5, ACCENT);
    let tx = rect.left() + 11.0;
    p.galley(pos2(tx, c - galley.size().y * 0.5), galley.clone(), mix(DIM, TEXT, 0.4));
    let lx = tx + galley.size().x + 8.0;
    if lx < rect.right() {
        p.line_segment([pos2(lx, c), pos2(rect.right(), c)], Stroke::new(1.0, Color32::from_white_alpha(18)));
    }
    ui.add_space(2.0);
}

/// Width of the label column of every inspector row.
pub const LABEL_W: f32 = 76.0;

/// A label in the fixed label column (so controls line up).
pub fn row_label(ui: &mut Ui, label: &str) {
    let rows = label.lines().count().clamp(1, 2) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(LABEL_W, 24.0 * rows), Sense::hover());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(12.5), DIM);
}

/// Three numbers on a row, X Y Z coloured like the axes. Returns whether any changed.
pub fn xyz(ui: &mut Ui, label: &str, v: &mut [f32; 3], speed: f64, range: std::ops::RangeInclusive<f32>, suffix: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.spacing_mut().item_spacing.x = 5.0;
        let avail = ui.available_width();
        row_label(ui, label);
        let fw = ((avail - LABEL_W - 5.0 - 3.0 * (10.0 + 5.0) - 2.0 * 5.0) / 3.0).max(44.0);
        for (i, c) in v.iter_mut().enumerate() {
            ui.add_sized([10.0, 22.0], egui::Label::new(egui::RichText::new(["X", "Y", "Z"][i]).color(AXIS[i]).size(12.0).strong()));
            changed |= field_sized(ui, [fw, 22.0], egui::DragValue::new(c).speed(speed).range(range.clone()).suffix(suffix).max_decimals(2)).changed();
        }
    });
    changed
}

/// A triangle that turns from pointing right to pointing down when `open`.
pub fn chevron(ui: &mut Ui, open: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    let ctx = ui.ctx();
    let t = ctx.animate_value_with_time(resp.id.with("open"), if open { 1.0 } else { 0.0 }, FAST);
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let c = rect.center();
    if hot > 0.01 {
        ui.painter().rect_filled(rect, 7.0, Color32::from_white_alpha((22.0 * hot) as u8));
    }
    let a = std::f32::consts::FRAC_PI_2 * gfx::smooth(t);
    let (s, co) = a.sin_cos();
    let pts = [vec2(-2.5, -4.5), vec2(-2.5, 4.5), vec2(3.5, 0.0)].map(|p| c + vec2(p.x * co - p.y * s, p.x * s + p.y * co));
    ui.painter().add(Shape::convex_polygon(pts.to_vec(), mix(DIM, Color32::WHITE, hot), Stroke::NONE));
    resp
}

/// A thin vertical divider between groups of buttons.
pub fn vsep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 24.0), Sense::hover());
    let x = rect.center().x;
    let p = ui.painter();
    p.line_segment([pos2(x - 0.5, rect.top() + 2.0), pos2(x - 0.5, rect.bottom() - 2.0)], Stroke::new(1.0, Color32::from_black_alpha(110)));
    p.line_segment([pos2(x + 0.5, rect.top() + 2.0), pos2(x + 0.5, rect.bottom() - 2.0)], Stroke::new(1.0, Color32::from_white_alpha(22)));
}

// ----- plates: cards, trays, panel backgrounds -----

/// Reserves a paint slot behind the content, lays the content out, then fills the slot.
fn plated<R>(ui: &mut Ui, margin: Margin, paint: impl FnOnce(&Buf, Rect), add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(Shape::Noop);
    let inner = egui::Frame::new().inner_margin(margin).show(ui, add_contents);
    let buf = Buf::default();
    paint(&buf, inner.response.rect);
    ui.painter().set(slot, buf.into_shape());
    inner
}

/// A rounded floating plate that sits over the picture.
pub struct Card;

/// A rounded floating card that sits over the picture.
pub fn card() -> Card {
    Card
}

impl Card {
    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        plated(ui, Margin::symmetric(12, 10), paint_card, add_contents)
    }
}

fn paint_card(out: &Buf, rect: Rect) {
    let r = 14.0;
    gfx::shadow(out, rect, r, &[(2, 4, 0, 90), (8, 18, 0, 90), (18, 36, 0, 60)]);
    gfx::vgrad(out, rect, r, Color32::from_rgba_unmultiplied(48, 50, 69, 246), Color32::from_rgba_unmultiplied(30, 31, 43, 248));
    out.put(Shape::rect_stroke(rect, r, Stroke::new(1.0, Color32::from_black_alpha(150)), StrokeKind::Inside));
    // the inner highlight border
    out.put(Shape::rect_stroke(rect.shrink(1.0), r - 1.0, Stroke::new(1.0, Color32::from_white_alpha(22)), StrokeKind::Inside));
    gfx::top_lip(out, rect.shrink(1.0), r, Color32::from_white_alpha(46));
}

/// A recessed tray for a group of buttons.
pub fn tray<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    plated(ui, Margin::same(3), paint_tray, add_contents)
}

fn paint_tray(out: &Buf, rect: Rect) {
    gfx::vgrad(out, rect, 13.0, INSET_TOP, INSET_BOTTOM);
    out.put(Shape::rect_stroke(rect, 13.0, Stroke::new(1.0, Color32::from_white_alpha(15)), StrokeKind::Inside));
    gfx::top_lip(out, rect, 13.0, Color32::from_black_alpha(120));
}

/// Which edge of a bar or panel faces the picture.
#[derive(Clone, Copy)]
pub enum Edge {
    Top,
    Bottom,
    Left,
}

/// Gives a bar or panel a soft vertical sheen and a hairline on the edge that faces the picture.
/// `rect` is the whole panel (the content rectangle plus its margins).
pub fn panel_sheen(ui: &Ui, rect: Rect, base: Color32, edge: Edge) {
    let p = ui.painter();
    gfx::vgrad(p, rect, 0.0, lit(base, 1.12), lit(base, 0.9));
    let (a, b, d) = match edge {
        Edge::Top => (rect.left_top(), rect.right_top(), vec2(0.0, 1.0)),
        Edge::Bottom => (rect.left_bottom() - vec2(0.0, 1.0), rect.right_bottom() - vec2(0.0, 1.0), vec2(0.0, -1.0)),
        Edge::Left => (rect.left_top(), rect.left_bottom(), vec2(1.0, 0.0)),
    };
    p.line_segment([a, b], Stroke::new(1.0, Color32::from_black_alpha(140)));
    p.line_segment([a + d, b + d], Stroke::new(1.0, Color32::from_white_alpha(14)));
}

/// The tile behind a thumbnail (material balls): sunken at rest, lifted when hovered, ringed when chosen.
pub fn tile(ui: &Ui, resp: &Response, rect: Rect, on: bool) {
    let ctx = ui.ctx();
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let on_t = ctx.animate_value_with_time(resp.id.with("on"), if on { 1.0 } else { 0.0 }, MEDIUM);
    let p = ui.painter();
    let r = rect.translate(vec2(0.0, -hot));
    if on_t > 0.01 {
        gfx::glow(p, r, 10.0, Color32::from_rgba_unmultiplied(150, 128, 255, (90.0 * on_t) as u8), 10);
    }
    gfx::vgrad(p, r, 10.0, mix(INSET_TOP, lit(CARD, 1.1), hot), mix(INSET_BOTTOM, CARD, hot));
    p.rect_stroke(r, 10.0, Stroke::new(1.0, mix(Color32::from_white_alpha(16), Color32::TRANSPARENT, on_t)), StrokeKind::Inside);
    if on_t > 0.01 {
        p.rect_stroke(r, 10.0, Stroke::new(2.0, mix(Color32::TRANSPARENT, ACCENT, on_t)), StrokeKind::Inside);
    }
    gfx::top_lip(p, r, 10.0, Color32::from_black_alpha(80));
}

/// A list row background: a soft wash when hovered, an accent plate with a left bar when chosen.
pub fn row_bg(ui: &Ui, resp: &Response, rect: Rect, selected: bool) {
    let ctx = ui.ctx();
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let sel = ctx.animate_value_with_time(resp.id.with("sel"), if selected { 1.0 } else { 0.0 }, MEDIUM);
    let p = ui.painter();
    if hot > 0.01 {
        p.rect_filled(rect, 9.0, Color32::from_white_alpha((14.0 * hot) as u8));
    }
    if sel > 0.01 {
        gfx::vgrad(
            p,
            rect,
            9.0,
            Color32::from_rgba_unmultiplied(112, 94, 220, (150.0 * sel) as u8),
            Color32::from_rgba_unmultiplied(70, 58, 150, (170.0 * sel) as u8),
        );
        p.rect_stroke(rect, 9.0, Stroke::new(1.0, mix(Color32::TRANSPARENT, ACCENT, sel)), StrokeKind::Inside);
        gfx::top_lip(p, rect, 9.0, Color32::from_white_alpha((50.0 * sel) as u8));
        p.rect_filled(Rect::from_min_max(pos2(rect.left() + 3.0, rect.top() + 6.0), pos2(rect.left() + 5.0, rect.bottom() - 6.0)), 1.0, ACCENT);
    }
}

// ----- fields, switches, sliders -----

fn inset_visuals(v: &mut egui::Visuals) {
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(20));
    for (w, fill) in [
        (&mut v.widgets.inactive, INSET_BOTTOM),
        (&mut v.widgets.hovered, mix(INSET_BOTTOM, HOVER, 0.35)),
        (&mut v.widgets.active, mix(INSET_BOTTOM, ACCENT_SOFT, 0.4)),
        (&mut v.widgets.open, INSET_BOTTOM),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = stroke;
        w.corner_radius = CornerRadius::same(8);
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(ACCENT_SOFT, ACCENT, 0.6));
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
}

/// Runs `add` with the sunken field look, then puts the normal look back. (Not `ui.scope`: that would
/// shift the vertical alignment of a row.)
fn sunken<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let saved = (ui.visuals().widgets.clone(), ui.spacing().interact_size, ui.spacing().button_padding);
    inset_visuals(ui.visuals_mut());
    ui.spacing_mut().interact_size.y = 22.0;
    ui.spacing_mut().button_padding = vec2(4.0, 1.0);
    let r = add(ui);
    ui.visuals_mut().widgets = saved.0;
    ui.spacing_mut().interact_size = saved.1;
    ui.spacing_mut().button_padding = saved.2;
    r
}

/// A number field or drag value with the sunken look.
pub fn field(ui: &mut Ui, widget: impl egui::Widget) -> Response {
    sunken(ui, |ui| ui.add(widget))
}

/// Like [`field`] with a fixed size.
pub fn field_sized(ui: &mut Ui, size: [f32; 2], widget: impl egui::Widget) -> Response {
    sunken(ui, |ui| ui.add_sized(size, widget))
}

/// A check box with a gradient fill and a check mark that fades in.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    let galley = text.into().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Button);
    let size = vec2(24.0 + galley.size().x, galley.size().y.max(22.0));
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let ctx = ui.ctx();
    let on = ctx.animate_value_with_time(resp.id.with("on"), if *checked { 1.0 } else { 0.0 }, FAST);
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - 9.0), Vec2::splat(18.0));
    let p = ui.painter();
    if on > 0.01 {
        gfx::glow(p, b, 6.0, Color32::from_rgba_unmultiplied(150, 128, 255, (70.0 * on) as u8), 8);
    }
    gfx::vgrad(p, b, 6.0, mix(INSET_TOP, BTN_ON_TOP, on), mix(INSET_BOTTOM, BTN_ON_BOTTOM, on));
    let edge = mix(mix(Color32::from_white_alpha(70), Color32::from_white_alpha(130), hot), BTN_ON_EDGE, on);
    p.rect_stroke(b, 6.0, Stroke::new(1.0, edge), StrokeKind::Inside);
    if on > 0.01 {
        let c = b.min;
        let pts = vec![c + vec2(4.5, 9.5), c + vec2(7.8, 12.8), c + vec2(13.5, 5.8)];
        p.add(Shape::line(pts, Stroke::new(2.2, Color32::from_white_alpha((255.0 * on) as u8))));
    }
    p.galley(pos2(rect.left() + 26.0, rect.center().y - galley.size().y * 0.5), galley, TEXT);
    resp
}

/// A toggle switch with a sliding knob and a label.
pub fn switch(ui: &mut Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> Response {
    let galley = text.into().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Button);
    let size = vec2(48.0 + galley.size().x, galley.size().y.max(24.0));
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let ctx = ui.ctx();
    let t = ctx.animate_value_with_time(resp.id.with("on"), if *on { 1.0 } else { 0.0 }, MEDIUM);
    let hot = ctx.animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    let track = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.0), vec2(38.0, 20.0));
    let p = ui.painter();
    gfx::vgrad(p, track, 10.0, mix(INSET_TOP, lit(BTN_ON_BOTTOM, 0.9), t), mix(INSET_BOTTOM, BTN_ON_TOP, t));
    p.rect_stroke(track, 10.0, Stroke::new(1.0, mix(Color32::from_white_alpha(30 + (40.0 * hot) as u8), BTN_ON_EDGE, t)), StrokeKind::Inside);
    let kx = lerp(track.left() + 10.0, track.right() - 10.0, gfx::smooth(t));
    let k = pos2(kx, track.center().y);
    p.circle_filled(k + vec2(0.0, 1.5), 8.5, Color32::from_black_alpha(80));
    gfx::ball(p, k, 7.5, vec2(-0.3, -0.4), &[(0.0, Color32::WHITE), (0.6, Color32::from_rgb(228, 226, 248)), (1.0, Color32::from_rgb(160, 156, 205))]);
    p.galley(pos2(rect.left() + 48.0, rect.center().y - galley.size().y * 0.5), galley, TEXT);
    resp
}

/// The value under pointer position `x` on a rail from `left` of width `width`.
pub fn value_at(x: f32, left: f32, width: f32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * gfx::unit((x - left) / width.max(1.0))
}

/// A slider: a sunken rail with a filled gradient and a glossy round thumb. Drag or click anywhere on it.
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width.max(40.0), 24.0), Sense::click_and_drag());
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return resp;
    }
    if !value.is_finite() {
        *value = lo;
    }
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 20.0, 8.0));
    if (resp.dragged() || resp.is_pointer_button_down_on() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let nv = value_at(pos.x, rail.left(), rail.width(), lo, hi);
        if nv != *value {
            *value = nv;
            resp.mark_changed();
        }
    }
    let t = gfx::unit((*value - lo) / (hi - lo));
    let grab = resp.dragged() || resp.is_pointer_button_down_on();
    let hot = ui.ctx().animate_value_with_time(resp.id.with("hot"), if resp.hovered() || grab { 1.0 } else { 0.0 }, FAST);
    let p = ui.painter();
    gfx::vgrad(p, rail, 4.0, INSET_TOP, INSET_BOTTOM);
    p.rect_stroke(rail, 4.0, Stroke::new(1.0, Color32::from_white_alpha(18)), StrokeKind::Inside);
    let cx = rail.left() + rail.width() * t;
    if cx - rail.left() > 1.0 {
        let fill = Rect::from_min_max(rail.min, pos2(cx, rail.max.y));
        gfx::glow(p, fill, 4.0, Color32::from_rgba_unmultiplied(150, 128, 255, (40.0 + 30.0 * hot) as u8), 8);
        gfx::vgrad(p, fill, 4.0, lit(ACCENT, 1.25), mix(ACCENT_SOFT, ACCENT, 0.55));
        gfx::top_lip(p, fill, 3.0, Color32::from_white_alpha(80));
    }
    let c = pos2(cx, rail.center().y);
    let r = 8.0 + 1.5 * hot;
    p.circle_filled(c + vec2(0.0, 2.0), r + 1.5, Color32::from_black_alpha(60));
    p.circle_filled(c + vec2(0.0, 1.0), r + 0.5, Color32::from_black_alpha(90));
    gfx::ball(p, c, r, vec2(-0.32, -0.42), &[(0.0, Color32::WHITE), (0.55, Color32::from_rgb(232, 230, 250)), (1.0, Color32::from_rgb(150, 146, 202))]);
    if hot > 0.01 {
        p.circle_stroke(c, r + 0.5, Stroke::new(1.5, Color32::from_rgba_unmultiplied(150, 128, 255, (255.0 * hot) as u8)));
    }
    p.circle_filled(c + vec2(-r * 0.32, -r * 0.4), r * 0.2, Color32::from_white_alpha(230));
    resp
}

/// A slider with its name in front (a fixed-width label column) and a number field after it.
pub fn labeled_slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, tip: &str) -> bool {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        let avail = ui.available_width();
        row_label(ui, label);
        let sw = (avail - LABEL_W - 56.0 - 2.0 * ui.spacing().item_spacing.x).clamp(70.0, 170.0);
        let (lo, hi) = (*range.start(), *range.end());
        let mut changed = false;
        let r = slider(ui, value, range.clone(), sw);
        changed |= r.changed();
        if !tip.is_empty() {
            r.on_hover_text(tip);
        }
        if lo.is_finite() && hi.is_finite() && hi > lo {
            let speed = f64::from(hi - lo) / 200.0;
            changed |= field_sized(ui, [56.0, 22.0], egui::DragValue::new(value).range(range).speed(speed).max_decimals(2)).changed();
        }
        changed
    })
    .inner
}

/// A colour button that only reports (and writes) a change the person made, so the colour is not
/// nudged by rounding every frame. Drawn with a ring and a lip so it reads as a glossy swatch.
pub fn color_edit(ui: &mut Ui, rgb: &mut [f32; 3]) -> bool {
    let mut c = *rgb;
    let resp = ui.color_edit_button_rgb(&mut c);
    let p = ui.painter();
    let hot = ui.ctx().animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, FAST);
    p.rect_stroke(resp.rect.expand(1.0), 8.0, Stroke::new(1.5, mix(Color32::from_white_alpha(70), ACCENT, hot)), StrokeKind::Outside);
    p.rect_stroke(resp.rect, 7.0, Stroke::new(1.0, Color32::from_black_alpha(120)), StrokeKind::Inside);
    gfx::top_lip(p, resp.rect, 7.0, Color32::from_white_alpha(70));
    if resp.changed() {
        *rgb = c;
        true
    } else {
        false
    }
}

/// Text anchored in a rect (used by the small status labels).
#[allow(dead_code)]
pub fn label_at(painter: &Painter, rect: Rect, text: &str, color: Color32) {
    painter.text(rect.left_center(), Align2::LEFT_CENTER, text, FontId::proportional(12.0), color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(c: Color32) -> u32 {
        c.r() as u32 + c.g() as u32 + c.b() as u32
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let l = |c: Color32| {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        };
        let (x, y) = (l(a), l(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn button_looks_get_lighter_when_hot_and_darker_when_pressed() {
        let (rest_top, rest_bottom, _) = button_look(0.0, 0.0, false);
        let (hot_top, hot_bottom, _) = button_look(1.0, 0.0, false);
        let (down_top, _, _) = button_look(0.0, 0.0, true);
        assert!(luma(hot_top) > luma(rest_top) && luma(hot_bottom) > luma(rest_bottom));
        assert!(luma(rest_top) > luma(rest_bottom), "lit from above");
        assert!(luma(down_top) < luma(rest_top), "pressed is pushed in");
        let (on_top, on_bottom, on_edge) = button_look(0.0, 1.0, false);
        assert_eq!((on_top, on_bottom), (BTN_ON_TOP, BTN_ON_BOTTOM));
        assert_eq!(on_edge, BTN_ON_EDGE);
        // ends of the animation are the plain states
        assert_eq!(button_look(0.0, 0.0, false).0, BTN_TOP);
    }

    #[test]
    fn labels_are_readable() {
        for bg in [PANEL, PANEL_DARK, CARD, INSET_BOTTOM] {
            assert!(contrast(DIM, bg) >= 4.5, "dim on {bg:?}: {}", contrast(DIM, bg));
            assert!(contrast(TEXT, bg) >= 7.0, "text on {bg:?}");
        }
        assert!(contrast(Color32::WHITE, BTN_ON_BOTTOM) >= 4.5, "white on the chosen button");
        assert!(contrast(Color32::WHITE, BTN_ON_TOP) >= 3.5);
        assert!(contrast(ACCENT, PANEL_DARK) >= 4.5, "accent text on the bar");
    }

    #[test]
    fn slider_positions_map_to_values() {
        assert_eq!(value_at(10.0, 10.0, 100.0, 0.0, 4.0), 0.0);
        assert_eq!(value_at(110.0, 10.0, 100.0, 0.0, 4.0), 4.0);
        assert_eq!(value_at(60.0, 10.0, 100.0, 0.0, 4.0), 2.0);
        assert_eq!(value_at(-500.0, 10.0, 100.0, 1.0, 3.0), 1.0);
        assert_eq!(value_at(500.0, 10.0, 100.0, 1.0, 3.0), 3.0);
        assert_eq!(value_at(f32::NAN, 10.0, 100.0, 1.0, 3.0), 1.0);
        assert!(value_at(5.0, 0.0, 0.0, -1.0, 1.0).is_finite());
    }

    #[test]
    fn widgets_draw_and_toggle_without_panicking() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut on = false;
        let mut check = true;
        let mut v = 0.5_f32;
        let mut bad = f32::NAN;
        let mut rgb = [0.2, 0.4, 0.6];
        let mut xyz3 = [1.0, 2.0, 3.0];
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                icon_button(ui, 34.0, "tip", on, |_, _, _| {});
                pill(ui, "あいう", on);
                pill_enabled(ui, "off", false, false);
                segmented(ui, "seg", &[("形", "a"), ("ざいしつ", ""), ("メルト", "c")], Some(1), true);
                segmented(ui, "seg2", &[("a", ""), ("b", "")], None, false);
                segmented(ui, "seg3", &[], None, false);
                title(ui, "Title");
                xyz(ui, "位置", &mut xyz3, 0.5, -10.0..=10.0, "");
                chevron(ui, on);
                vsep(ui);
                checkbox(ui, &mut check, "check");
                switch(ui, &mut on, "switch");
                slider(ui, &mut v, 0.0..=1.0, 120.0);
                slider(ui, &mut bad, 0.0..=1.0, 120.0);
                slider(ui, &mut v, 1.0..=1.0, 120.0);
                labeled_slider(ui, "前の組と\nとけあう", &mut v, 0.0..=1.0, "tip");
                color_edit(ui, &mut rgb);
                card().show(ui, |ui| {
                    ui.label("hello");
                });
                tray(ui, |ui| {
                    ui.label("tray");
                });
                menu_button(ui, "menu", true, |ui| {
                    ui.label("x");
                });
                panel_sheen(ui, ui.max_rect(), PANEL, Edge::Left);
                panel_sheen(ui, ui.max_rect(), PANEL, Edge::Top);
                panel_sheen(ui, ui.max_rect(), PANEL, Edge::Bottom);
            });
            out.textures_delta.clear();
        }
        assert!(bad.is_finite(), "a NaN value is repaired");
    }

    #[test]
    fn slider_follows_the_pointer() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut v = 0.0_f32;
        let frame = |ctx: &egui::Context, events: Vec<egui::Event>, v: &mut f32| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
                events,
                ..Default::default()
            };
            let mut rect = Rect::NOTHING;
            let mut out = ctx.run_ui(input, |ui| {
                rect = slider(ui, v, 0.0..=10.0, 200.0).rect;
            });
            out.textures_delta.clear();
            rect
        };
        let rect = frame(&ctx, vec![], &mut v);
        let at = pos2(rect.left() + 10.0 + 0.5 * (rect.width() - 20.0), rect.center().y);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], &mut v);
        frame(
            &ctx,
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }],
            &mut v,
        );
        assert!((v - 5.0).abs() < 0.5, "pressing in the middle sets about half: {v}");
    }
}
