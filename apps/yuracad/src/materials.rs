//! A library of ready-made materials and the little ball that previews one.

use egui::{Color32, Painter, Pos2, Stroke, vec2};

use crate::model::Material;
use crate::style::gfx;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Basic,
    Metal,
    Glass,
    Glow,
    Fun,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Basic, Category::Metal, Category::Glass, Category::Glow, Category::Fun];
    pub fn label(self) -> &'static str {
        match self {
            Category::Basic => "Basic",
            Category::Metal => "Metal",
            Category::Glass => "Glass",
            Category::Glow => "Glow",
            Category::Fun => "Magic",
        }
    }
}

pub struct Preset {
    pub name: &'static str,
    pub category: Category,
    pub material: Material,
}

const fn m(color: [f32; 3], roughness: f32, metallic: f32, glow: f32, glass: f32, iridescence: f32, coat: f32) -> Material {
    Material { color, roughness, metallic, glow, glass, iridescence, coat, translucency: 0.0 }
}

/// The same material with light seeping through it.
const fn t(base: Material, translucency: f32) -> Material {
    Material { translucency, ..base }
}

const fn p(name: &'static str, category: Category, material: Material) -> Preset {
    Preset { name, category, material }
}

use Category::{Basic, Fun, Glass, Glow, Metal};

static PRESETS: &[Preset] = &[
    // basic
    p("Clay", Basic, m([0.93, 0.62, 0.50], 0.85, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Matte", Basic, m([0.80, 0.80, 0.82], 0.95, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Plastic", Basic, m([0.92, 0.55, 0.42], 0.45, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Glossy plastic", Basic, m([0.40, 0.62, 0.96], 0.18, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Rubber", Basic, m([0.14, 0.14, 0.16], 0.70, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Wax", Basic, t(m([0.98, 0.88, 0.62], 0.50, 0.0, 0.0, 0.0, 0.0, 0.0), 0.6)),
    p("Milk", Basic, t(m([0.97, 0.96, 0.93], 0.80, 0.0, 0.0, 0.0, 0.0, 0.0), 0.3)),
    p("Ink", Basic, m([0.09, 0.09, 0.11], 0.30, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Strawberry", Basic, m([0.96, 0.40, 0.52], 0.28, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Mint", Basic, m([0.55, 0.90, 0.78], 0.45, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Sky", Basic, m([0.50, 0.75, 0.98], 0.35, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Lavender", Basic, m([0.72, 0.64, 0.96], 0.45, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Lemon", Basic, m([0.98, 0.90, 0.46], 0.40, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Pink", Basic, t(m([0.99, 0.78, 0.86], 0.55, 0.0, 0.0, 0.0, 0.0, 0.0), 0.25)),
    p("Nut", Basic, m([0.50, 0.32, 0.20], 0.65, 0.0, 0.0, 0.0, 0.0, 0.0)),
    p("Stone", Basic, m([0.52, 0.52, 0.54], 0.90, 0.0, 0.0, 0.0, 0.0, 0.0)),
    // metal
    p("Gold", Metal, m([1.00, 0.78, 0.35], 0.25, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Rose gold", Metal, m([0.97, 0.64, 0.56], 0.28, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Silver", Metal, m([0.93, 0.93, 0.95], 0.22, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Chrome", Metal, m([0.86, 0.88, 0.92], 0.07, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Copper", Metal, m([0.95, 0.55, 0.40], 0.30, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Bronze", Metal, m([0.80, 0.55, 0.30], 0.38, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Titanium", Metal, m([0.62, 0.62, 0.66], 0.42, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Rusty iron", Metal, m([0.55, 0.32, 0.22], 0.85, 0.7, 0.0, 0.0, 0.0, 0.0)),
    p("Gunmetal", Metal, m([0.25, 0.26, 0.30], 0.30, 1.0, 0.0, 0.0, 0.0, 0.0)),
    p("Verdigris", Metal, m([0.35, 0.72, 0.62], 0.55, 0.8, 0.0, 0.0, 0.0, 0.0)),
    // glass
    p("Clear glass", Glass, m([0.97, 0.99, 1.00], 0.03, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Frosted glass", Glass, m([0.95, 0.97, 1.00], 0.45, 0.0, 0.0, 0.85, 0.0, 0.0)),
    p("Blue glass", Glass, m([0.45, 0.70, 1.00], 0.04, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Green glass", Glass, m([0.45, 0.90, 0.58], 0.04, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Pink glass", Glass, m([1.00, 0.62, 0.78], 0.05, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Ruby", Glass, m([0.90, 0.10, 0.18], 0.03, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Amber", Glass, m([1.00, 0.62, 0.15], 0.04, 0.0, 0.0, 1.0, 0.0, 0.0)),
    p("Diamond", Glass, m([0.98, 0.99, 1.00], 0.02, 0.0, 0.0, 1.0, 0.25, 0.0)),
    p("Droplet", Glass, t(m([0.90, 0.97, 1.00], 0.02, 0.0, 0.0, 0.92, 0.0, 0.6), 0.3)),
    // glow
    p("Neon pink", Glow, m([1.00, 0.30, 0.65], 0.4, 0.0, 1.6, 0.0, 0.0, 0.0)),
    p("Neon blue", Glow, m([0.25, 0.65, 1.00], 0.4, 0.0, 1.6, 0.0, 0.0, 0.0)),
    p("Neon green", Glow, m([0.35, 1.00, 0.45], 0.4, 0.0, 1.5, 0.0, 0.0, 0.0)),
    p("Lamp", Glow, m([1.00, 0.82, 0.50], 0.5, 0.0, 1.8, 0.0, 0.0, 0.0)),
    p("Lava", Glow, m([1.00, 0.38, 0.10], 0.6, 0.0, 1.4, 0.0, 0.0, 0.0)),
    p("Firefly", Glow, m([0.80, 1.00, 0.30], 0.5, 0.0, 1.2, 0.0, 0.0, 0.0)),
    p("Ice", Glow, t(m([0.70, 0.92, 1.00], 0.2, 0.0, 0.4, 0.4, 0.0, 0.0), 0.5)),
    // fun
    p("Soap bubble", Fun, m([0.95, 0.98, 1.00], 0.03, 0.0, 0.0, 0.92, 1.0, 0.0)),
    p("Hologram", Fun, m([0.85, 0.88, 0.95], 0.18, 0.8, 0.0, 0.0, 1.0, 0.0)),
    p("Car paint", Fun, m([0.86, 0.06, 0.10], 0.25, 0.0, 0.0, 0.0, 0.0, 1.0)),
    p("Pearl", Fun, m([0.97, 0.95, 0.97], 0.30, 0.0, 0.0, 0.0, 0.35, 0.8)),
    p("Jewel beetle", Fun, m([0.05, 0.35, 0.22], 0.22, 0.8, 0.0, 0.0, 0.9, 0.0)),
    p("Candy", Fun, m([1.00, 0.45, 0.70], 0.30, 0.0, 0.0, 0.0, 0.0, 1.0)),
    p("Jelly", Fun, t(m([1.00, 0.55, 0.20], 0.10, 0.0, 0.0, 0.62, 0.0, 0.8), 0.8)),
    p("Opal", Fun, m([0.95, 0.96, 1.00], 0.20, 0.0, 0.0, 0.35, 0.85, 0.4)),
    p("Night sky", Fun, m([0.10, 0.12, 0.35], 0.20, 0.5, 0.2, 0.0, 0.6, 0.7)),
    p("Marshmallow", Fun, t(m([1.00, 0.92, 0.94], 0.75, 0.0, 0.0, 0.0, 0.0, 0.0), 0.7)),
    p("Chocolate", Fun, m([0.30, 0.16, 0.10], 0.35, 0.0, 0.0, 0.0, 0.0, 0.7)),
    p("Mint ice", Fun, t(m([0.62, 0.95, 0.85], 0.55, 0.0, 0.0, 0.0, 0.0, 0.3), 0.4)),
];

pub fn presets() -> &'static [Preset] {
    PRESETS
}

/// sRGB 0..1 to a colour (NaN safe, clamped).
pub fn color32(c: [f32; 3]) -> Color32 {
    let f = |v: f32| if v.is_finite() { (v.clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 };
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

fn shade(c: [f32; 3], k: f32) -> Color32 {
    color32([c[0] * k, c[1] * k, c[2] * k])
}

fn lighten(c: [f32; 3], t: f32) -> Color32 {
    color32([c[0] + (1.0 - c[0]) * t, c[1] + (1.0 - c[1]) * t, c[2] + (1.0 - c[2]) * t])
}

/// A small shaded ball showing roughly what the material looks like.
pub fn swatch(painter: &Painter, centre: Pos2, radius: f32, m: &Material) {
    let r = radius.max(2.0);
    let c = m.color;
    let rough = m.roughness.clamp(0.0, 1.0);
    let metal = m.metallic.clamp(0.0, 1.0);
    let glass = m.glass.clamp(0.0, 1.0);
    let glow = m.glow.clamp(0.0, 4.0);
    let irid = m.iridescence.clamp(0.0, 1.0);
    let coat = m.coat.clamp(0.0, 1.0);
    // the glow reaches outside the ball
    if glow > 0.05 {
        for (k, a) in [(1.7, 14.0), (1.45, 24.0), (1.22, 40.0)] {
            let alpha = (a * glow.min(2.0)).min(90.0) as u8;
            painter.circle_filled(centre, r * k, color32(c).gamma_multiply(alpha as f32 / 255.0));
        }
    }
    if glass > 0.05 {
        // a grey checker behind, so see-through reads
        painter.circle_filled(centre, r, Color32::from_gray(58));
        painter.circle_filled(centre + vec2(-r * 0.32, -r * 0.3), r * 0.38, Color32::from_gray(84));
        painter.circle_filled(centre + vec2(r * 0.34, r * 0.34), r * 0.34, Color32::from_gray(84));
    }
    // the body: dark at the edge, lighter toward the lit side
    let body_alpha = 1.0 - glass * 0.72;
    let base = if metal > 0.5 { shade(c, 0.55) } else { shade(c, 0.62) };
    // smooth rings from the light spot (top left) out to the dark rim
    gfx::ball(
        painter,
        centre,
        r,
        vec2(-0.34, -0.38),
        &[(0.0, lighten(c, 0.22).gamma_multiply(body_alpha)), (0.45, shade(c, 0.92).gamma_multiply(body_alpha)), (1.0, base.gamma_multiply(body_alpha))],
    );
    if metal > 0.3 {
        // a bright horizon band
        let y = centre.y + r * 0.1;
        painter.line_segment(
            [Pos2::new(centre.x - r * 0.78, y), Pos2::new(centre.x + r * 0.78, y)],
            Stroke::new(r * 0.16, lighten(c, 0.55).gamma_multiply(0.55 * metal)),
        );
    }
    if irid > 0.05 {
        for (dx, dy, col) in [(-0.35, -0.3, [1.0, 0.45, 0.85]), (0.35, 0.05, [0.35, 0.95, 1.0]), (-0.05, 0.45, [1.0, 0.95, 0.4])] {
            painter.circle_filled(centre + vec2(r * dx, r * dy), r * 0.42, color32(col).gamma_multiply(0.42 * irid));
        }
    }
    if glass > 0.05 {
        // a bright rim and the crescent of bent light
        painter.circle_stroke(centre, r - 0.5, Stroke::new((r * 0.12).max(1.0), Color32::from_white_alpha((150.0 * glass) as u8)));
        painter.circle_stroke(
            centre + vec2(r * 0.12, r * 0.14),
            r * 0.62,
            Stroke::new((r * 0.10).max(0.8), Color32::from_white_alpha((70.0 * glass) as u8)),
        );
    }
    // the highlight gets smaller and sharper as it gets smoother
    let hl = r * (0.14 + 0.30 * rough);
    let alpha = (255.0 * (0.95 - 0.55 * rough)) as u8;
    painter.circle_filled(centre + vec2(-r * 0.34, -r * 0.38), hl, Color32::from_white_alpha(alpha));
    if coat > 0.05 {
        painter.circle_filled(centre + vec2(-r * 0.30, -r * 0.34), r * 0.12, Color32::from_white_alpha((255.0 * coat) as u8));
    }
    if glow > 0.05 {
        painter.circle_filled(centre, r * 0.35, Color32::from_white_alpha((70.0 * glow.min(2.0)) as u8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_is_big_and_valid() {
        let list = presets();
        assert!(list.len() >= 40);
        let mut names: Vec<_> = list.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), list.len(), "names are unique");
        for cat in Category::ALL {
            assert!(list.iter().filter(|p| p.category == cat).count() >= 6, "{cat:?}");
        }
        for pr in list {
            let mut c = pr.material;
            c.sanitize();
            assert_eq!(c, pr.material, "{} is already in range", pr.name);
        }
    }

    #[test]
    fn colours_are_clamped() {
        assert_eq!(color32([2.0, -1.0, f32::NAN]), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn every_swatch_draws() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for pr in presets() {
                swatch(&painter, Pos2::new(40.0, 40.0), 14.0, &pr.material);
            }
        });
        out.textures_delta.clear();
    }
}
