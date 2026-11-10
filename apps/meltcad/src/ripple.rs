//! Non-destructive Ripple surface modifier shared by picking, rendering and export.
//! The CPU evaluation is mirrored in shader.wgsl (ripple_offset); keep both in step.
//!
//! The original shape stays parametric. No preview mesh is built or subdivided.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RippleMode {
    #[default]
    Concentric,
    Linear,
    Random,
}

impl RippleMode {
    pub fn id(self) -> f32 {
        match self {
            Self::Concentric => 1.0,
            Self::Linear => 2.0,
            Self::Random => 3.0,
        }
    }
}

/// Sizes and wavelengths are expressed in the shape's unscaled local millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ripple {
    pub mode: RippleMode,
    pub height: f32,
    pub spacing: f32,
    /// Maximum number of rings, or full straight waves across the central band.
    pub count: u8,
    /// Direction of straight/random waves in the shape's local XY plane, in degrees.
    pub direction: f32,
    /// Fraction (0..1) of the wavelength region over which ripples fade out.
    pub edge_fade: f32,
    /// Recess the original surface only: the final solid cannot expand past the original.
    pub preserve_outline: bool,
    /// Number of smooth frequency layers (1..4), not a polygon count.
    pub detail: u8,
}

impl Default for Ripple {
    fn default() -> Self {
        Self {
            mode: RippleMode::Concentric,
            height: 2.0,
            spacing: 12.0,
            count: 5,
            direction: 0.0,
            edge_fade: 0.35,
            preserve_outline: false,
            detail: 2,
        }
    }
}

fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[allow(dead_code)]
impl Ripple {
    pub fn concentric() -> Self {
        Self::default()
    }
    pub fn linear() -> Self {
        Self { mode: RippleMode::Linear, count: 6, ..Self::default() }
    }
    pub fn random() -> Self {
        Self { mode: RippleMode::Random, detail: 4, ..Self::default() }
    }

    /// Prevent malformed scenes from introducing NaNs or extreme work on the GPU.
    pub fn sanitize(&mut self) {
        self.height = if self.height.is_finite() { self.height.clamp(0.0, 40.0) } else { 2.0 };
        self.spacing = if self.spacing.is_finite() { self.spacing.clamp(0.5, 200.0) } else { 12.0 };
        self.count = self.count.clamp(1, 32);
        self.direction = if self.direction.is_finite() { self.direction.rem_euclid(360.0) } else { 0.0 };
        self.edge_fade = if self.edge_fade.is_finite() { self.edge_fade.clamp(0.05, 1.0) } else { 0.35 };
        self.detail = self.detail.clamp(1, 4);
    }

    /// Conservative distance estimate: only the zero set must be exact for marching/meshing.
    /// The larger Lipschitz bound accounts for the maximum local wave and edge-fade slopes.
    /// The same displacement is evaluated for CPU selection and GPU ray tracing.
    pub fn adjust(&self, base: f32, q: [f32; 3], half: [f32; 3], min_scale: f32) -> f32 {
        if self.height <= 0.0 {
            return base;
        }
        let amplitude = self.height * min_scale;
        if base.abs() > amplitude * 2.0 {
            // For exterior points the surface cannot be closer than the original minus
            // its maximum outward displacement. No trigonometry for distant samples.
            return if base > 0.0 && !self.preserve_outline { base - amplitude } else { base };
        }
        let (sn, cs) = self.direction.to_radians().sin_cos();
        let u = q[0] * cs + q[1] * sn;
        let v = q[1] * cs - q[0] * sn;
        let radius = q[0].hypot(q[1]);
        let k = std::f32::consts::TAU / self.spacing;
        let support = f32::from(self.count) * self.spacing * if self.mode == RippleMode::Linear { 0.5 } else { 1.0 };
        let distance = if self.mode == RippleMode::Linear { u.abs() } else { radius };
        let fade_width = (support * self.edge_fade).max(0.001);
        let mut envelope = 1.0 - smooth01((distance - (support - fade_width)) / fade_width);
        if self.preserve_outline {
            let hx = half[0].max(0.01);
            let hy = half[1].max(0.01);
            let edge = (q[0].abs() / hx).max(q[1].abs() / hy);
            envelope *= 1.0 - smooth01((edge - 0.68) / 0.32);
        }
        let (mut wave, mut weight) = match self.mode {
            RippleMode::Concentric => ((radius * k).sin(), 1.0),
            RippleMode::Linear => ((u * k).sin(), 1.0),
            RippleMode::Random => (((0.84 * u + 0.34 * v) * k).sin(), 1.0),
        };
        if self.detail >= 2 {
            let a = if self.mode == RippleMode::Random {
                (-0.31 * u + 1.23 * v) * k + 0.8
            } else {
                (if self.mode == RippleMode::Linear { u } else { radius }) * k * 2.0 + 0.3
            };
            wave += 0.15 * a.sin();
            weight += 0.15;
        }
        if self.detail >= 3 {
            let a = if self.mode == RippleMode::Random {
                (1.37 * u - 0.71 * v) * k + 1.1
            } else {
                (if self.mode == RippleMode::Linear { u } else { radius }) * k * 3.0 + 1.1
            };
            wave += 0.07 * a.sin();
            weight += 0.07;
        }
        if self.detail >= 4 {
            let a = if self.mode == RippleMode::Random {
                (-1.73 * u - 0.48 * v) * k + 2.0
            } else {
                (if self.mode == RippleMode::Linear { u } else { radius }) * k * 4.0 + 2.0
            };
            wave += 0.035 * a.sin();
            weight += 0.035;
        }
        let wave = wave / weight;
        let delta = self.height * envelope * if self.preserve_outline { (1.0 - wave) * 0.5 } else { -wave };
        (base + delta * min_scale) / self.safety(half)
    }

    /// How much the ripple shortens the distance where it bends the surface (1 where it does
    /// not). A shell measured on the rippled field must use the same factor, or its wall is too
    /// thick. `base` is the distance before the ripple.
    pub fn shell_shortening(&self, base: f32, half: [f32; 3], min_scale: f32) -> f32 {
        if self.height <= 0.0 || base.abs() > self.height * min_scale * 2.0 {
            return 1.0;
        }
        let safety = self.safety(half);
        if safety.is_finite() && safety > 1.0 { 1.0 / safety } else { 1.0 }
    }

    /// The distance bound `adjust` divides by: the local wave, fade and edge slopes.
    fn safety(&self, half: [f32; 3]) -> f32 {
        let k = std::f32::consts::TAU / self.spacing;
        let support = f32::from(self.count) * self.spacing * if self.mode == RippleMode::Linear { 0.5 } else { 1.0 };
        let fade_width = (support * self.edge_fade).max(0.001);
        let edge_slope = if self.preserve_outline { 6.0 / half[0].min(half[1]).max(0.01) } else { 0.0 };
        1.0 + self.height * (3.0 * k + 2.0 / fade_width + edge_slope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_deterministic_and_finite() {
        for mode in [RippleMode::Concentric, RippleMode::Linear, RippleMode::Random] {
            for detail in 1..=4 {
                let r = Ripple { mode, detail, ..Ripple::default() };
                for x in -25..=25 {
                    let p = [x as f32 * 0.9, x as f32 * 0.3, 2.5];
                    let a = r.adjust(0.0, p, [30.0, 30.0, 3.0], 1.0);
                    assert!(a.is_finite() && a.abs() <= r.height, "{mode:?} {detail}: {a}");
                    assert_eq!(a, r.adjust(0.0, p, [30.0, 30.0, 3.0], 1.0));
                }
            }
        }
    }

    #[test]
    fn preserve_outline_only_carves_and_fades_at_edges() {
        let r = Ripple { preserve_outline: true, count: 8, ..Ripple::default() };
        for x in -30..=30 {
            let d = r.adjust(0.0, [x as f32, 0.0, 3.0], [30.0, 30.0, 3.0], 1.0);
            assert!(d >= -1e-6, "{x}: {d}");
        }
        assert!(r.adjust(0.0, [30.0, 0.0, 3.0], [30.0; 3], 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_height_is_identity_and_invalid_values_are_clamped() {
        let mut r =
            Ripple { height: f32::INFINITY, spacing: -10.0, count: 0, direction: f32::NAN, edge_fade: -10.0, detail: 255, ..Ripple::default() };
        r.sanitize();
        assert_eq!(r.height, 2.0);
        assert_eq!(r.spacing, 0.5);
        assert_eq!(r.count, 1);
        assert_eq!(r.direction, 0.0);
        assert_eq!(r.edge_fade, 0.05);
        assert_eq!(r.detail, 4);
        r.height = 0.0;
        assert_eq!(r.adjust(10.0, [0.0; 3], [1.0; 3], 1.0), 10.0);
    }
}
