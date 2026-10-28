//! Parametric, print-oriented flower vases built on the existing Revolve SDF.
//! A single simple profile encloses a solid bottom, an open mouth, and both wall faces.
//! Angular waves are interpreted by cad.rs and shader_cad.wgsl; no Boolean difference
//! or high-resolution remeshing is needed while a parameter is dragged.

use serde::{Deserialize, Serialize};

use crate::model::{Corner, Kind, Material, Shape};

pub const PROFILE_STEPS: usize = 27;
pub const RIM_STEPS: usize = 6;
/// Two sampled sides, a rounded mouth and a closed base, within MAX_OUTLINE = 64.
pub const PROFILE_POINTS: usize = (PROFILE_STEPS + 1) + RIM_STEPS + PROFILE_STEPS + 2;
pub const FDM_WALL_WARNING_MM: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VaseParams {
    pub height: f32,
    pub diameter: f32,
    pub mouth_diameter: f32,
    pub base_diameter: f32,
    pub wall: f32,
    /// Longitudinal undulations in the vase silhouette (0..1).
    pub side_wave: f32,
    /// Narrowing around the upper third (0..1).
    pub waist: f32,
    /// Full, rounded belly and soft elliptical lip (0..1).
    pub roundness: f32,
    /// Integer periodicity of the circumferential waves (0 disables).
    pub waves: u8,
    /// Radial amplitude, mm. Both wall faces receive exactly the same deformation.
    pub wave_strength: f32,
}

impl Default for VaseParams {
    fn default() -> Self {
        Self {
            height: 132.0, diameter: 92.0, mouth_diameter: 49.0,
            base_diameter: 54.0, wall: 2.4, side_wave: 0.16,
            waist: 0.20, roundness: 0.78, waves: 5, wave_strength: 1.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Design {
    Flow,
    Ripple,
    Pebble,
}

impl Design {
    pub const ALL: [Design; 3] = [Self::Flow, Self::Ripple, Self::Pebble];

    pub fn name(self) -> &'static str {
        match self {
            Self::Flow => "Organic Vase — Flow",
            Self::Ripple => "Organic Vase — Ripple",
            Self::Pebble => "Organic Vase — Pebble",
        }
    }

    pub fn params(self) -> VaseParams {
        match self {
            Self::Flow => VaseParams::default(),
            Self::Ripple => VaseParams {
                height: 155.0, diameter: 102.0, mouth_diameter: 46.0,
                base_diameter: 51.0, wall: 2.8, side_wave: 0.53,
                waist: 0.52, roundness: 0.52, waves: 8, wave_strength: 2.7,
            },
            Self::Pebble => VaseParams {
                height: 104.0, diameter: 115.0, mouth_diameter: 71.0,
                base_diameter: 76.0, wall: 2.8, side_wave: 0.33,
                waist: 0.26, roundness: 0.95, waves: 6, wave_strength: 1.6,
            },
        }
    }

    pub fn material(self) -> Material {
        match self {
            Self::Flow => Material::plain([0.89, 0.72, 0.61], 0.29, 0.0),
            Self::Ripple => Material::plain([0.56, 0.72, 0.89], 0.26, 0.0),
            Self::Pebble => Material::plain([0.83, 0.80, 0.70], 0.34, 0.0),
        }
    }
}

impl VaseParams {
    /// Bounds guard against self-intersecting profiles, negative inner radii, huge meshes,
    /// NaNs and malformed values supplied by old or hand-edited project files.
    pub fn sanitize(&mut self) {
        let safe = |x: f32, fallback: f32, lo: f32, hi: f32| {
            if x.is_finite() { x.clamp(lo, hi) } else { fallback }
        };
        self.height = safe(self.height, 132.0, 36.0, 380.0);
        self.diameter = safe(self.diameter, 92.0, 28.0, 240.0);
        self.mouth_diameter = safe(self.mouth_diameter, 49.0, 12.0, self.diameter);
        self.base_diameter = safe(self.base_diameter, 54.0, 14.0, self.diameter);
        // A wall cannot exceed the mouth radius and must leave a hollow interior.
        let max_wall = (0.5 * self.mouth_diameter.min(self.base_diameter) - 2.5).min(16.0);
        self.wall = safe(self.wall, 2.4, 0.4, max_wall.max(0.4));
        self.side_wave = safe(self.side_wave, 0.16, 0.0, 1.0);
        self.waist = safe(self.waist, 0.2, 0.0, 1.0);
        self.roundness = safe(self.roundness, 0.78, 0.0, 1.0);
        self.waves = self.waves.min(16);
        let free = (0.5 * self.mouth_diameter.min(self.base_diameter) - self.wall - 2.0).max(0.0);
        self.wave_strength = safe(self.wave_strength, 1.4, 0.0, 6.0_f32.min(free));
    }

    /// A smooth silhouette with exact nominal base/mouth radii. The near-horizontal
    /// end derivatives avoid sharp seams with the rounded lip and flat base.
    pub fn radius(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let ease = t * t * (3.0 - 2.0 * t);
        let floor = self.base_diameter * 0.5;
        let mouth = self.mouth_diameter * 0.5;
        let max_radius = self.diameter * 0.5;
        let envelope = (std::f32::consts::PI * t).sin().powi(2);
        let belly = (max_radius - (floor + mouth) * 0.5).max(0.0)
            * envelope.powf(1.0 + 0.9 * (1.0 - self.roundness));
        let neck = self.waist * max_radius * 0.22
            * (-((t - 0.74) / 0.14).powi(2)).exp() * envelope;
        let ripple = self.side_wave * max_radius * 0.065
            * (std::f32::consts::TAU * 2.5 * t).sin() * envelope;
        ((1.0 - ease) * floor + ease * mouth + belly - neck + ripple)
            .max(self.wall + 2.0)
    }

    /// The outer/inner vertical outline is one CCW/CW simple polygon, including the
    /// flat bottom, inner floor and the top rim. The segment along the axis is ignored
    /// by the existing Revolve distance function, but closes the polygon sign correctly.
    pub fn profile(&self) -> Vec<[f32; 2]> {
        let h = self.height;
        let rim_h = (self.wall * 0.5 * (0.16 + 0.84 * self.roundness)).max(0.08);
        let lip_z = h - rim_h;
        let bottom_h = (self.wall * 1.25).max(1.0);
        let mut points = Vec::with_capacity(PROFILE_POINTS);
        for i in 0..=PROFILE_STEPS {
            let z = lip_z * (i as f32 / PROFILE_STEPS as f32);
            points.push([self.radius(z / h), z - h * 0.5]);
        }
        let lip_radius = self.radius(lip_z / h);
        // Elliptical (not flat or sharp) lip from exterior to interior.
        for i in 1..=RIM_STEPS {
            let angle = std::f32::consts::PI * (i as f32 / RIM_STEPS as f32);
            let r = lip_radius - self.wall * 0.5 * (1.0 - angle.cos());
            let z = lip_z + rim_h * angle.sin();
            points.push([r, z - h * 0.5]);
        }
        // Inner wall is offset radially from the outer wall; the cavity stops
        // at the inner floor, so the bottom is watertight and not hollowed through.
        for i in 1..=PROFILE_STEPS {
            let z = lip_z - (lip_z - bottom_h) * (i as f32 / PROFILE_STEPS as f32);
            points.push([self.radius(z / h) - self.wall, z - h * 0.5]);
        }
        points.push([0.0, bottom_h - h * 0.5]);
        points.push([0.0, -h * 0.5]);
        points
    }

    /// Sampled estimate of the narrowest NORMAL wall thickness. A radial distance
    /// alone would overestimate steep/wavy regions. This is a warning, not a
    /// certified geometric minimum or a substitute for slicer inspection.
    pub fn wall_report(&self) -> ThicknessReport {
        let mut report = ThicknessReport { minimum_mm: self.wall, height_mm: 0.0 };
        for i in 2..=62 {
            let t = i as f32 / 64.0;
            let delta = 0.0005;
            let slope_z = (self.radius((t + delta).min(1.0)) -
                self.radius((t - delta).max(0.0))) / (2.0 * delta * self.height);
            let envelope = angular_envelope(t);
            let az_slope = self.wave_strength * (self.waves as f32) * envelope
                / (self.radius(t) - self.wall).max(4.0);
            let z_slope = self.wave_strength * (16.0 / self.height);
            let thickness = self.wall / (1.0 + slope_z.powi(2)
                + az_slope.powi(2) + z_slope.powi(2)).sqrt();
            if thickness < report.minimum_mm {
                report = ThicknessReport { minimum_mm: thickness, height_mm: t * self.height };
            }
        }
        report
    }

    #[cfg(test)]
    pub fn estimated_min_wall(&self) -> f32 {
        self.wall_report().minimum_mm
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThicknessReport {
    pub minimum_mm: f32,
    /// Where the thinnest sampled section occurs, measured from the vase's bottom.
    pub height_mm: f32,
}

/// The exact same envelope is used for the CPU and GPU SDF radial warp.
/// It vanishes smoothly at the bottom, but retains half strength near the lip.
pub fn angular_envelope(z01: f32) -> f32 {
    let t = z01.clamp(0.0, 1.0);
    let u = (t / 0.12).clamp(0.0, 1.0);
    let eased = u * u * (3.0 - 2.0 * u);
    eased * (0.5 + 0.5 * (std::f32::consts::PI * t).sin().powi(2))
}

/// Radial remapping for Revolve; zero-wave profiles still use the ORIGINAL SDF,
/// preserving compatibility with ordinary sketches and legacy models.
pub fn radial_warp(r: f32, q: [f32; 3], count: f32, strength: f32, height: f32) -> (f32, f32) {
    let n = if count.is_finite() { count.round().clamp(0.0, 16.0) } else { 0.0 };
    let amp = if strength.is_finite() { strength.clamp(0.0, 6.0) } else { 0.0 };
    if n < 1.0 || amp <= 0.0 {
        return (r, 1.0);
    }
    let h = if height.is_finite() { height.clamp(1.0, 380.0) } else { 100.0 };
    let t = (q[2] / h + 0.5).clamp(0.0, 1.0);
    let gate_t = (r / 5.0).clamp(0.0, 1.0);
    let gate = gate_t * gate_t * (3.0 - 2.0 * gate_t);
    let phase = n * q[1].atan2(q[0]) + std::f32::consts::TAU * 0.4 * t;
    let warped = r - amp * angular_envelope(t) * gate * phase.sin();
    // A conservative upper bound on the warp gradient avoids over-stepping
    // small waves in the sphere tracer / sparse mesher. Zero set is unchanged.
    let safety = 1.0 / (1.0 + amp * (n / 5.0 + 17.0 / h + 0.30));
    (warped, safety)
}

/// Apply on every edit and after file loading. Shape remains a standard Revolve,
/// editable as a regular sketch after explicitly clearing its parametric preset.
pub fn apply(shape: &mut Shape) {
    let Some(mut values) = shape.vase else { return };
    if shape.kind != Kind::Revolve {
        shape.vase = None;
        return;
    }
    values.sanitize();
    shape.vase = Some(values);
    shape.outline = values.profile();
    shape.params = [360.0, values.waves as f32, values.wave_strength, values.height];
    // The shell comes from the closed profile, not Shape::shell (which would also
    // hollow out the bottom), and rounding is in the generated profile.
    shape.shell = 0.0;
    shape.round = 0.0;
    shape.corner = Corner::Round;
}

pub fn new(design: Design) -> Shape {
    let values = design.params();
    let mut shape = Shape::new(Kind::Revolve, [0.0, 0.0, values.height * 0.5], design.material());
    shape.name = design.name().to_string();
    shape.blend = 0.0;
    shape.vase = Some(values);
    apply(&mut shape);
    shape
}


/// The inspector edits the parametric source rather than directly mutating a dense
/// vertex mesh. Changes regenerate at most 63 profile points; viewport quality
/// adapts independently and final STL export always samples the full SDF.
pub fn inspector(ui: &mut egui::Ui, shape: &mut Shape) {
    let Some(old) = shape.vase else { return };
    let mut value = old;
    ui.separator();
    ui.label(egui::RichText::new("Organic Vase｜有機的な花瓶").strong().size(14.0));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("形のサンプル").small());
        for design in Design::ALL {
            let name = match design {
                Design::Flow => "Flow",
                Design::Ripple => "Ripple",
                Design::Pebble => "Pebble",
            };
            if ui.button(name).on_hover_text(design.name()).clicked() {
                value = design.params();
            }
        }
    });
    egui::Grid::new("organic_vase_mm").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let mut field = |label: &str, v: &mut f32, max: f32| {
            ui.label(label);
            ui.add(egui::DragValue::new(v).speed(0.4).range(0.4..=max).suffix(" mm").max_decimals(2));
            ui.end_row();
        };
        field("高さ", &mut value.height, 380.0);
        field("最大直径", &mut value.diameter, 240.0);
        field("口径", &mut value.mouth_diameter, 240.0);
        field("底の幅", &mut value.base_diameter, 240.0);
        field("壁厚", &mut value.wall, 16.0);
        field("波の高さ", &mut value.wave_strength, 6.0);
    });
    ui.add(egui::Slider::new(&mut value.side_wave, 0.0..=1.0).text("側面のうねり"));
    ui.add(egui::Slider::new(&mut value.waist, 0.0..=1.0).text("くびれ"));
    ui.add(egui::Slider::new(&mut value.roundness, 0.0..=1.0).text("丸み"));
    ui.add(egui::Slider::new(&mut value.waves, 0..=16).text("円周の波の数"));

    if value != old {
        // Keep the vase standing on the same floor while its height changes.
        let base_z = shape.pos[2] - old.height * 0.5;
        value.sanitize();
        shape.vase = Some(value);
        shape.pos[2] = base_z + value.height * 0.5;
        apply(shape);
    }
    let v = shape.vase.unwrap_or(value);
    // Non-uniform object scaling can only reduce physical wall thickness:
    // the smallest scale is a conservative lower bound for print warnings.
    let thickness = v.wall_report();
    let minimum = thickness.minimum_mm
        * shape.scale.into_iter().fold(f32::INFINITY, f32::min).max(0.01);
    if minimum < FDM_WALL_WARNING_MM {
        ui.colored_label(
            egui::Color32::from_rgb(237, 174, 78),
            format!("⚠ 推定最小肉厚 {:.2} mm（底から約 {:.0} mm、0.8 mm未満）", minimum, thickness.height_mm),
        );
        ui.small("壁厚を増やすか、波・くびれを弱めてください。スライサーでも確認してください。");
    } else {
        ui.label(format!("推定最小肉厚 {:.2} mm（底から約 {:.0} mm）", minimum, thickness.height_mm));
    }
    ui.small("底は閉じて中は空洞です。実際の印刷可否はノズル径・スライサー設定で確認してください。");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Scene};
    use crate::sdf::Field;

    fn scene(shape: Shape) -> Scene {
        let mut g = Group::new("Vase");
        g.shapes.push(shape);
        Scene { groups: vec![g], ..Scene::default() }
    }

    #[test]
    fn three_profiles_are_simple_and_round_trip_in_existing_shape_format() {
        for design in Design::ALL {
            let s = new(design);
            let p = s.vase.unwrap();
            assert_eq!(s.kind, Kind::Revolve);
            assert_eq!(s.outline.len(), PROFILE_POINTS);
            assert!(s.outline.len() <= crate::model::MAX_OUTLINE);
            assert!(crate::sketch::is_simple(&s.outline), "{design:?} has self-intersections");
            assert!(s.outline.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
            assert_eq!(s.params, [360.0, p.waves as f32, p.wave_strength, p.height]);
            let mut sc = scene(s);
            let before = sc.clone();
            sc.sanitize();
            assert_eq!(before, sc, "sanitizing a valid preset must not change it");
            let loaded = Scene::from_json(&sc.to_json()).expect("round trip");
            assert_eq!(loaded, sc);
        }
    }

    #[test]
    fn floor_is_solid_and_mouth_is_open_for_all_three_vases() {
        for design in Design::ALL {
            let s = new(design);
            let v = s.vase.unwrap();
            let f = Field::new(&scene(s));
            assert!(f.distance([0.0, 0.0, v.wall * 0.55]) < 0.0, "solid bottom {design:?}");
            assert!(f.distance([0.0, 0.0, -2.0]) > 0.0, "no open bottom {design:?}");
            assert!(f.distance([0.0, 0.0, v.height * 0.55]) > 0.0, "hollow cavity {design:?}");
            assert!(f.distance([0.0, 0.0, v.height + 2.0]) > 0.0, "open mouth {design:?}");
            let r = v.radius(0.5);
            let z = v.height * 0.5;
            // At theta=0 the wall is radially shifted by the same wave as the
            // SDF. Sample its actual middle, not the unwarped revolution radius.
            let shift = v.wave_strength * angular_envelope(0.5)
                * (std::f32::consts::TAU * 0.4 * 0.5).sin();
            assert!(f.distance([r - v.wall * 0.5 + shift, 0.0, z]) < 0.0, "solid wall {design:?}");
        }
    }

    #[test]
    fn controls_modify_smooth_profile_and_ripple_field_without_remeshing() {
        let mut s = new(Design::Flow);
        let old = s.outline.clone();
        let mut values = s.vase.unwrap();
        values.height += 38.0;
        values.mouth_diameter += 8.0;
        values.base_diameter += 2.0;
        values.diameter += 12.0;
        values.side_wave = 0.85;
        values.waist = 0.70;
        values.roundness = 0.20;
        values.wall = 2.0;
        values.waves = 10;
        values.wave_strength = 2.5;
        s.vase = Some(values);
        apply(&mut s);
        assert_ne!(s.outline, old);
        assert_eq!(s.outline.len(), PROFILE_POINTS);
        assert!(crate::sketch::is_simple(&s.outline));
        let q = [values.diameter * 0.47, 0.0, 0.0];
        let (r1, factor) = radial_warp(q[0], q, 10.0, 2.5, values.height);
        assert_ne!(r1, q[0]);
        assert!(factor > 0.0 && factor <= 1.0);
        assert_eq!(radial_warp(20.0, q, 0.0, 2.5, values.height), (20.0, 1.0));
        assert_eq!(radial_warp(20.0, q, 5.0, 0.0, values.height), (20.0, 1.0));
    }

    #[test]
    fn inspector_draws_without_graphics_device_and_keeps_parameters() {
        let mut shape = new(Design::Ripple);
        let before = shape.vase;
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                inspector(ui, &mut shape);
            }).textures_delta.clear();
        }
        assert_eq!(shape.vase, before);
        assert!(shape.outline.len() <= crate::model::MAX_OUTLINE);
    }

    #[test]
    fn narrow_normals_trigger_a_print_warning() {
        let mut v = Design::Ripple.params();
        v.wall = 0.45;
        v.waves = 16;
        v.wave_strength = 4.0;
        v.sanitize();
        assert!(v.estimated_min_wall() < FDM_WALL_WARNING_MM);
        let mut normal = Design::Flow.params();
        normal.waves = 0;
        normal.wave_strength = 0.0;
        assert!(normal.estimated_min_wall() > 0.8);
    }

    /// Expensive 3D-print validation, opt-in for CI/Windows Release:
    /// cargo test -p meltcad organic_vase_stl_is_watertight -- --ignored --nocapture
    #[test]
    #[ignore]
    fn organic_vase_stl_is_watertight() {
        use crate::export_job::{self, Options};
        use crate::export_mesh::Progress;
        use crate::export_formats::Format;
        for design in Design::ALL {
            let mut sh = new(design);
            let mut v = sh.vase.unwrap();
            // Keep a small mesh for CI while retaining >3 tolerance cells of wall.
            v.height = 55.0;
            v.diameter = 45.0;
            v.mouth_diameter = 23.0;
            v.base_diameter = 28.0;
            v.wall = 2.7;
            v.wave_strength = 0.8;
            sh.vase = Some(v);
            apply(&mut sh);
            sh.pos[2] = v.height * 0.5;
            let options = Options { format: Format::Stl, tolerance: 0.65, ..Options::default() };
            let built = export_job::build(&scene(sh), None, &options, &Progress::new()).expect("mesh generation");
            assert!(built.closed, "{design:?} not watertight");
            assert!(!built.surface.mesh.triangles.is_empty());
            let stl = crate::export_formats::stl_binary(&built.surface.mesh, design.name());
            let triangles = crate::export_formats::tests::read_stl_binary(&stl);
            assert_eq!(triangles.len(), built.surface.mesh.triangles.len());
        }
    }
}
