//! Non-destructive, shape-local geometric modifiers.
//!
//! Twist is a volume-preserving mapping: every slice perpendicular to the
//! selected axis is rigidly rotated. Original primitive, profile, mesh volume
//! and scene records are never baked or overwritten. GPU preview and CPU
//! SDF/meshing evaluate the same inverse mapping.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    #[default]
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
    pub fn label(self) -> &'static str {
        match self { Axis::X => "X", Axis::Y => "Y", Axis::Z => "Z" }
    }
    pub fn index(self) -> usize {
        match self { Axis::X => 0, Axis::Y => 1, Axis::Z => 2 }
    }
    pub fn transverse(self) -> (usize, usize) {
        // Cyclic ordering makes positive angles right-handed.
        match self {
            Axis::X => (1, 2),
            Axis::Y => (2, 0),
            Axis::Z => (0, 1),
        }
    }
    pub fn shader_id(self) -> f32 { self.index() as f32 + 1.0 }
}

/// An extensible modifier slot; other deformation types can share the same
/// inverse-map, bounding-box and distance-safety API later.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deformation {
    Twist(Twist),
}

impl Deformation {
    pub fn sanitize(&mut self) {
        match self { Deformation::Twist(t) => t.sanitize() }
    }
    pub fn expanded_half(self, half: [f32; 3]) -> [f32; 3] {
        match self { Deformation::Twist(t) => t.expanded_half(half) }
    }
    pub fn inverse(self, q: [f32; 3], half: [f32; 3]) -> [f32; 3] {
        match self { Deformation::Twist(t) => t.inverse(q, half) }
    }
    /// A conservative signed-distance step for the non-isometric warp.
    /// Safety only for marching rays, never baked into the Boolean SDF.
    pub fn ray_step_safety(self, half: [f32; 3]) -> f32 {
        match self { Deformation::Twist(t) => t.ray_step_safety(half) }
    }
    pub fn gpu_parameters(self, half: [f32; 3]) -> ([f32; 4], [f32; 4]) {
        match self { Deformation::Twist(t) => t.gpu_parameters(half) }
    }
}

/// All settings are local to the original shape (before its scale/rotation).
/// begin/end measure the affected fraction along the axis from bottom (0)
/// to top (1). `lower` and `upper` control the twist amount at each end:
/// by default lower=0, upper=1, so 90 degrees means top +90 from bottom.
/// Sign is controlled with `reverse`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Twist {
    pub axis: Axis,
    pub reverse: bool,
    pub degrees: f32,
    pub begin: f32,
    pub end: f32,
    pub lower: f32,
    pub upper: f32,
}

impl Default for Twist {
    fn default() -> Self {
        Self { axis: Axis::Z, reverse: false, degrees: 90.0, begin: 0.0, end: 1.0, lower: 0.0, upper: 1.0 }
    }
}

impl Twist {
    pub fn preset(degrees: f32) -> Self {
        Self { degrees, ..Self::default() }
    }

    pub fn sanitize(&mut self) {
        let valid = |v: f32, default: f32, lo: f32, hi: f32| -> f32 {
            if v.is_finite() { v.clamp(lo, hi) } else { default }
        };
        self.degrees = valid(self.degrees, 90.0, 0.0, 1440.0);
        self.begin = valid(self.begin, 0.0, 0.0, 0.98);
        self.end = valid(self.end, 1.0, self.begin + 0.02, 1.0);
        self.lower = valid(self.lower, 0.0, 0.0, 2.0);
        self.upper = valid(self.upper, 1.0, 0.0, 2.0);
    }

    fn unit_fraction(self, q: [f32; 3], half: [f32; 3]) -> f32 {
        let axis = self.axis.index();
        let h = half[axis].max(0.01);
        let along = (q[axis] / (2.0 * h) + 0.5 - self.begin) / (self.end - self.begin).max(0.02);
        along.clamp(0.0, 1.0)
    }

    /// Positive or negative radians of twist at this local axial coordinate.
    pub fn angle_at(self, q: [f32; 3], half: [f32; 3]) -> f32 {
        let t = self.unit_fraction(q, half);
        let smooth = t * t * (3.0 - 2.0 * t);
        let factor = self.lower + (self.upper - self.lower) * smooth;
        self.degrees.to_radians() * if self.reverse { -factor } else { factor }
    }

    fn rotated(self, q: [f32; 3], half: [f32; 3], inverse: bool) -> [f32; 3] {
        let theta = self.angle_at(q, half) * if inverse { -1.0 } else { 1.0 };
        let (sn, cs) = theta.sin_cos();
        let (a, b) = self.axis.transverse();
        let mut out = q;
        out[a] = cs * q[a] - sn * q[b];
        out[b] = sn * q[a] + cs * q[b];
        out
    }

    pub fn forward(self, q: [f32; 3], half: [f32; 3]) -> [f32; 3] {
        self.rotated(q, half, false)
    }
    pub fn inverse(self, q: [f32; 3], half: [f32; 3]) -> [f32; 3] {
        self.rotated(q, half, true)
    }

    /// Twist never changes radial distance or the axial coordinate. Expand
    /// only the two transverse bounding-box components to enclose *all*
    /// possible rotations of the original half-extent.
    pub fn expanded_half(self, half: [f32; 3]) -> [f32; 3] {
        let (a, b) = self.axis.transverse();
        let radius = half[a].hypot(half[b]);
        let mut out = half;
        out[a] = radius;
        out[b] = radius;
        out
    }

    /// A smoothstep's maximum derivative is 1.5. The inverse map's local
    /// Lipschitz bound is <= 1 + transverse_radius * abs(d angle / d axis).
    /// No cross-section is flattened: Jacobian determinant is 1, while SDF
    /// step length is conservatively shortened for safe tracing/meshing.
    pub fn max_radians_per_mm(self, half: [f32; 3]) -> f32 {
        let h = half[self.axis.index()].max(0.01);
        let height = (2.0 * h * (self.end - self.begin).max(0.02)).max(0.01);
        1.5 * self.degrees.to_radians().abs() * (self.upper - self.lower).abs() / height
    }

    pub fn distance_safety(self, q: [f32; 3], half: [f32; 3]) -> f32 {
        let (a, b) = self.axis.transverse();
        let max_original_radius = half[a].hypot(half[b]);
        let query_radius = q[a].hypot(q[b]);
        1.0 / (1.0 + self.max_radians_per_mm(half) * max_original_radius.max(query_radius))
    }

    /// Worst useful radius for a ray near the shape's inflated bounding
    /// sphere. Using a factor at ~2x the original transverse extent makes
    /// the marching step conservative near twisted solids, including blends.
    /// It is deliberately not used by SDF Boolean or the export mesher.
    pub fn ray_step_safety(self, half: [f32; 3]) -> f32 {
        let (a, b) = self.axis.transverse();
        let radius = 2.0 * half[a].hypot(half[b]) + 8.0;
        1.0 / (1.0 + self.max_radians_per_mm(half) * radius)
    }

    /// Two extra vec4 entries in the existing GPU scene-list texture.
    /// slot 0: axis ID (0 = disabled), signed degrees, begin, end
    /// slot 1: lower/upper factors, original half-height, original max radius.
    pub fn gpu_parameters(self, half: [f32; 3]) -> ([f32; 4], [f32; 4]) {
        let (a, b) = self.axis.transverse();
        ([self.axis.shader_id(), if self.reverse { -self.degrees } else { self.degrees }, self.begin, self.end],
            [self.lower, self.upper, half[self.axis.index()].max(0.01), half[a].hypot(half[b])])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{a} versus {b}");
    }

    #[test]
    fn presets_and_expanded_bounds() {
        let angles: Vec<f32> = [90.0, 180.0, 360.0].map(|a| Twist::preset(a).degrees).into_iter().collect();
        assert_eq!(angles, vec![90.0, 180.0, 360.0]);
        let h = Twist::default().expanded_half([10.0, 20.0, 30.0]);
        approx(h[0], 10.0_f32.hypot(20.0));
        approx(h[1], 10.0_f32.hypot(20.0));
        approx(h[2], 30.0);
    }

    #[test]
    fn every_axis_preserves_section_radius_and_roundtrips() {
        for axis in Axis::ALL {
            let t = Twist { axis, degrees: 360.0, begin: 0.1, end: 0.9, ..Twist::default() };
            let half = [18.0, 12.0, 20.0];
            let (a, b) = axis.transverse();
            for p in [[4.0, 7.0, -8.0], [12.0, -3.0, 6.0], [-5.0, 2.0, 3.0]] {
                let q = t.forward(p, half);
                let back = t.inverse(q, half);
                for k in 0..3 { approx(back[k], p[k]); }
                approx(q[a].hypot(q[b]), p[a].hypot(p[b]));
                approx(q[axis.index()], p[axis.index()]);
            }
        }
    }

    #[test]
    fn smooth_range_has_zero_derivative_at_its_ends() {
        let t = Twist { begin: 0.2, end: 0.8, ..Twist::default() };
        let h = [10.0, 10.0, 30.0];
        let x = |z| t.angle_at([4.0, 2.0, z], h);
        let lo = -30.0 + 60.0 * t.begin;
        let hi = -30.0 + 60.0 * t.end;
        approx(x(lo), 0.0);
        approx(x(hi), std::f32::consts::FRAC_PI_2);
        assert!((x(lo + 0.001) - x(lo)).abs() < 1e-6);
        assert!((x(hi) - x(hi - 0.001)).abs() < 1e-6);
    }

    #[test]
    fn backwards_direction_and_end_factors() {
        let t = Twist { degrees: 180.0, reverse: true, lower: 0.5, upper: 1.5, ..Twist::default() };
        let half = [20.0; 3];
        approx(t.angle_at([0.0, 0.0, -20.0], half), -std::f32::consts::FRAC_PI_2);
        approx(t.angle_at([0.0, 0.0, 20.0], half), -3.0 * std::f32::consts::PI / 2.0);
    }

    #[test]
    fn ray_safety_shortens_steps_without_changing_the_surface() {
        let h = [15.0, 12.0, 20.0];
        let untwisted = Twist { lower: 1.0, upper: 1.0, ..Twist::default() };
        approx(untwisted.ray_step_safety(h), 1.0);
        for degree in [90.0, 180.0, 360.0] {
            let t = Twist::preset(degree);
            let factor = t.ray_step_safety(h);
            assert!(factor > 0.0 && factor < 1.0, "twist {degree}: {factor}");
        }
    }

    #[test]
    fn sanitizes_hostile_numbers() {
        let mut t = Twist { degrees: f32::NAN, begin: 2.0, end: -2.0, lower: f32::INFINITY, upper: -1.0, ..Twist::default() };
        t.sanitize();
        assert!(t.degrees.is_finite());
        assert!(t.end - t.begin >= 0.02 - 1e-5);
        assert!((0.0..=2.0).contains(&t.upper));
        for axis in Axis::ALL {
            let x = Twist { axis, ..t };
            assert!(x.distance_safety([200.0, 0.0, 0.0], [1.0; 3]) > 0.0);
        }
    }
}
