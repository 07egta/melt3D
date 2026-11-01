//! A small FDM slicer: triangles to layers, walls, rectilinear infill and G-code text.
//!
//! Everything is plain numbers in millimetres, so it is tested without a printer. Inputs are
//! hostile: every count is capped and bad numbers are errors, never panics.
//!
//! Limits (stated, not hidden): walls use a mitre offset, so they suit shapes without very sharp
//! inner corners; solids that overlap each other are sliced with the even-odd rule, so overlapping
//! parts cancel; there are no supports, no bridges and no top/bottom skins (infill runs on every
//! layer).

use std::collections::HashMap;
use std::fmt::Write as _;

use solidcraft_geom::{Vec2, Vec3};

/// One triangle.
pub type Tri = [Vec3; 3];

/// Most layers one slice may make.
pub const MAX_LAYERS: usize = 20_000;
/// Most triangles one slice may read.
pub const MAX_TRIANGLES: usize = 2_000_000;
/// Most perimeter walls.
pub const MAX_WALLS: u32 = 8;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SliceError {
    #[error("{0} must be a number greater than zero")]
    BadNumber(&'static str),
    #[error("there is nothing to slice (no triangles, or the shape is flat)")]
    Empty,
    #[error("too many triangles (up to {MAX_TRIANGLES})")]
    TooManyTriangles,
    #[error("too many layers (up to {MAX_LAYERS}); use a thicker layer height")]
    TooManyLayers,
}

/// One layer: the outlines of the material at its middle height.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Height of the layer's top, measured from the bottom of the shape.
    pub top: f64,
    /// Closed outlines. Outer loops run counter-clockwise, holes clockwise.
    pub loops: Vec<Vec<Vec2>>,
    /// Outlines that did not close: the shape has holes in its surface here.
    pub open: u32,
}

/// The result of slicing a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Sliced {
    pub layer_height: f64,
    pub layers: Vec<Layer>,
}

impl Sliced {
    /// How many outlines failed to close, over all layers. More than zero means the shape is not
    /// watertight and will print with gaps.
    pub fn open_outlines(&self) -> u32 {
        self.layers.iter().map(|l| l.open).fold(0, u32::saturating_add)
    }
}

fn finite3(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

/// Cuts `tris` into layers `layer_height` thick, the shape's lowest point sitting at height 0.
pub fn slice(tris: &[Tri], layer_height: f64) -> Result<Sliced, SliceError> {
    if !(layer_height > 1e-3) || !layer_height.is_finite() {
        return Err(SliceError::BadNumber("layer height"));
    }
    if tris.len() > MAX_TRIANGLES {
        return Err(SliceError::TooManyTriangles);
    }
    let mut zmin = f64::INFINITY;
    let mut zmax = f64::NEG_INFINITY;
    for t in tris {
        for v in t {
            if !finite3(*v) {
                return Err(SliceError::BadNumber("coordinate"));
            }
            zmin = zmin.min(v.z);
            zmax = zmax.max(v.z);
        }
    }
    let count = ((zmax - zmin) / layer_height).floor();
    if !(count >= 1.0) {
        return Err(SliceError::Empty);
    }
    if count > MAX_LAYERS as f64 {
        return Err(SliceError::TooManyLayers);
    }
    let count = count as usize;
    let mut layers = Vec::with_capacity(count);
    for i in 0..count {
        let z = zmin + (i as f64 + 0.5) * layer_height;
        let (loops, open) = loops_at(tris, z);
        layers.push(Layer { top: (i as f64 + 1.0) * layer_height, loops, open });
    }
    Ok(Sliced { layer_height, layers })
}

type Key = (i64, i64);

fn key(p: Vec2) -> Key {
    ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64)
}

/// The point where the edge `a`-`b` meets height `z`; the same for either order of `a` and `b`.
fn cross_edge(a: Vec3, b: Vec3, z: f64) -> Vec2 {
    let (a, b) = if (a.x, a.y, a.z).partial_cmp(&(b.x, b.y, b.z)) == Some(std::cmp::Ordering::Greater) { (b, a) } else { (a, b) };
    let dz = b.z - a.z;
    let t = if dz.abs() < 1e-12 { 0.0 } else { ((z - a.z) / dz).clamp(0.0, 1.0) };
    Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn loops_at(tris: &[Tri], z: f64) -> (Vec<Vec<Vec2>>, u32) {
    let mut segs: Vec<(Vec2, Vec2)> = Vec::new();
    for t in tris {
        // A vertex exactly on the cut counts as above it, so no triangle is cut twice at a corner.
        let above = [t[0].z > z - 1e-9, t[1].z > z - 1e-9, t[2].z > z - 1e-9];
        if above[0] == above[1] && above[1] == above[2] {
            continue;
        }
        let mut pts: Vec<Vec2> = Vec::with_capacity(2);
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            if above[i] != above[j] {
                pts.push(cross_edge(t[i], t[j], z));
            }
        }
        if let [a, b] = pts[..]
            && key(a) != key(b)
        {
            segs.push((a, b));
        }
    }
    let mut by_end: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        by_end.entry(key(*a)).or_default().push(i);
        by_end.entry(key(*b)).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops: Vec<Vec<Vec2>> = Vec::new();
    let mut open = 0u32;
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (first, mut at) = segs[start];
        let mut pts = vec![first];
        let mut closed = false;
        for _ in 0..=segs.len() {
            if key(at) == key(first) {
                closed = true;
                break;
            }
            pts.push(at);
            let next = by_end.get(&key(at)).and_then(|c| c.iter().copied().find(|k| !used[*k]));
            let Some(k) = next else { break };
            used[k] = true;
            let (a, b) = segs[k];
            at = if key(a) == key(at) { b } else { a };
        }
        if closed && pts.len() >= 3 {
            loops.push(pts);
        } else {
            open = open.saturating_add(1);
        }
    }
    (orient(loops), open)
}

/// Signed area: positive when the loop runs counter-clockwise.
pub fn area(l: &[Vec2]) -> f64 {
    let mut s = 0.0;
    for (i, a) in l.iter().enumerate() {
        let b = l[(i + 1) % l.len()];
        s += a.x * b.y - b.x * a.y;
    }
    s * 0.5
}

fn contains(l: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = l.len().wrapping_sub(1);
    for (i, a) in l.iter().enumerate() {
        let b = l[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Outer loops counter-clockwise, loops inside an odd number of others (holes) clockwise.
fn orient(mut loops: Vec<Vec<Vec2>>) -> Vec<Vec<Vec2>> {
    let depths: Vec<usize> = loops
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let p = l.first().copied().unwrap_or(Vec2::ZERO);
            loops.iter().enumerate().filter(|(j, o)| *j != i && contains(o, p)).count()
        })
        .collect();
    for (l, d) in loops.iter_mut().zip(depths) {
        let ccw = area(l) > 0.0;
        if ccw != (d % 2 == 0) {
            l.reverse();
        }
    }
    loops
}

/// Moves a loop sideways by `d` into the material (positive) or out of it (negative).
/// `None` when the loop collapses.
pub fn offset(l: &[Vec2], d: f64) -> Option<Vec<Vec2>> {
    let n = l.len();
    if n < 3 {
        return None;
    }
    let mut out: Vec<Vec2> = Vec::with_capacity(n);
    for i in 0..n {
        let p = l[i];
        let prev = l[(i + n - 1) % n];
        let next = l[(i + 1) % n];
        let n1 = left_normal(prev, p)?;
        let n2 = left_normal(p, next)?;
        let denom = (1.0 + n1.dot(n2)).max(0.25);
        let m = Vec2::new((n1.x + n2.x) / denom, (n1.y + n2.y) / denom);
        out.push(Vec2::new(p.x + m.x * d, p.y + m.y * d));
    }
    // A loop that was pushed past its own middle comes out turned inside out, which the sign of
    // its area cannot show (a square turned half way round is still counter-clockwise). It shows
    // in the edges: one that now points the opposite way means the loop has collapsed.
    for i in 0..n {
        let (a, b) = (l[i], l[(i + 1) % n]);
        let (c, e) = (out[i], out[(i + 1) % n]);
        if (b.x - a.x) * (e.x - c.x) + (b.y - a.y) * (e.y - c.y) <= 0.0 {
            return None;
        }
    }
    if area(&out).abs() < 1e-6 {
        return None;
    }
    Some(out)
}

fn left_normal(a: Vec2, b: Vec2) -> Option<Vec2> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len < 1e-9 { None } else { Some(Vec2::new(-dy / len, dx / len)) }
}

/// Parallel lines `spacing` apart at `angle` (radians), cut to the inside of `loops` (even-odd).
pub fn infill(loops: &[Vec<Vec2>], spacing: f64, angle: f64) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    if !(spacing > 1e-3) || loops.is_empty() {
        return out;
    }
    let (s, c) = angle.sin_cos();
    let rot = |p: Vec2, a: f64, b: f64| Vec2::new(p.x * a - p.y * b, p.x * b + p.y * a);
    let turned: Vec<Vec<Vec2>> = loops.iter().map(|l| l.iter().map(|p| rot(*p, c, -s)).collect()).collect();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in turned.iter().flatten() {
        lo = lo.min(p.y);
        hi = hi.max(p.y);
    }
    if !(hi > lo) || (hi - lo) / spacing > 20_000.0 {
        return out;
    }
    let mut y = (lo / spacing).ceil() * spacing + spacing * 0.5;
    while y < hi {
        let mut xs: Vec<f64> = Vec::new();
        for l in &turned {
            for (i, a) in l.iter().enumerate() {
                let b = l[(i + 1) % l.len()];
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks_exact(2) {
            if pair[1] - pair[0] > 0.05 {
                out.push((rot(Vec2::new(pair[0], y), c, s), rot(Vec2::new(pair[1], y), c, s)));
            }
        }
        y += spacing;
    }
    out
}

/// Printer and filament settings.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintConfig {
    pub line_width: f64,
    pub filament_diameter: f64,
    pub nozzle_temp: f64,
    pub bed_temp: f64,
    pub walls: u32,
    /// Distance between infill lines; zero turns infill off.
    pub infill_spacing: f64,
    pub print_speed: f64,
    pub travel_speed: f64,
    pub retract: f64,
    /// Where the middle of the shape goes on the bed.
    pub bed_center: (f64, f64),
    /// Bed size; the shape must fit.
    pub bed_size: (f64, f64),
    pub start_gcode: String,
    pub end_gcode: String,
}

impl Default for PrintConfig {
    /// A generic Marlin-style printer with a 180 x 180 mm bed (Bambu A1 mini size).
    /// The start and end G-code are plain placeholders: Bambu machines need their own.
    fn default() -> Self {
        PrintConfig {
            line_width: 0.42,
            filament_diameter: 1.75,
            nozzle_temp: 220.0,
            bed_temp: 60.0,
            walls: 2,
            infill_spacing: 3.0,
            print_speed: 60.0,
            travel_speed: 150.0,
            retract: 0.8,
            bed_center: (90.0, 90.0),
            bed_size: (180.0, 180.0),
            start_gcode: "G28 ; home all axes".to_string(),
            end_gcode: "M84 ; motors off".to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GcodeError {
    #[error("{0} must be a number greater than zero")]
    BadNumber(&'static str),
    #[error("nothing to print")]
    Empty,
    #[error("the shape does not fit on the bed ({0:.0} x {1:.0} mm needed)")]
    TooBig(f64, f64),
}

struct Writer<'a> {
    cfg: &'a PrintConfig,
    text: String,
    at: Vec2,
    e_per_mm: f64,
    layer_h: f64,
    retracted: bool,
}

impl Writer<'_> {
    fn line(&mut self, s: &str) {
        self.text.push_str(s);
        self.text.push('\n');
    }
    fn place(&self, p: Vec2) -> Vec2 {
        Vec2::new(p.x + self.cfg.bed_center.0, p.y + self.cfg.bed_center.1)
    }
    fn travel(&mut self, to: Vec2) {
        let to = self.place(to);
        if !self.retracted {
            let _ = writeln!(self.text, "G1 E-{:.3} F2400", self.cfg.retract);
            self.retracted = true;
        }
        let _ = writeln!(self.text, "G0 X{:.3} Y{:.3} F{:.0}", to.x, to.y, self.cfg.travel_speed * 60.0);
        self.at = to;
    }
    fn print_to(&mut self, to: Vec2) {
        let to = self.place(to);
        if self.retracted {
            let _ = writeln!(self.text, "G1 E{:.3} F2400", self.cfg.retract);
            self.retracted = false;
        }
        let len = (to.x - self.at.x).hypot(to.y - self.at.y);
        let e = len * self.cfg.line_width * self.layer_h * self.e_per_mm;
        let _ = writeln!(self.text, "G1 X{:.3} Y{:.3} E{:.5} F{:.0}", to.x, to.y, e, self.cfg.print_speed * 60.0);
        self.at = to;
    }
    fn path(&mut self, pts: &[Vec2], closed: bool) {
        let Some(first) = pts.first().copied() else { return };
        self.travel(first);
        for p in pts.iter().skip(1) {
            self.print_to(*p);
        }
        if closed {
            self.print_to(first);
        }
    }
}

/// Turns sliced layers into G-code text.
pub fn gcode(sliced: &Sliced, cfg: &PrintConfig) -> Result<String, GcodeError> {
    for (name, v) in [
        ("line width", cfg.line_width),
        ("filament diameter", cfg.filament_diameter),
        ("print speed", cfg.print_speed),
        ("travel speed", cfg.travel_speed),
    ] {
        if !(v > 0.0) || !v.is_finite() {
            return Err(GcodeError::BadNumber(name));
        }
    }
    let (mut lo, mut hi) = (Vec2::new(f64::INFINITY, f64::INFINITY), Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in sliced.layers.iter().flat_map(|l| l.loops.iter().flatten()) {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(hi.x >= lo.x) {
        return Err(GcodeError::Empty);
    }
    let (w, d) = (hi.x - lo.x, hi.y - lo.y);
    if w > cfg.bed_size.0 || d > cfg.bed_size.1 {
        return Err(GcodeError::TooBig(w, d));
    }
    let centre = Vec2::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let area_filament = std::f64::consts::PI * (cfg.filament_diameter * 0.5).powi(2);
    let mut w_ = Writer { cfg, text: String::new(), at: Vec2::ZERO, e_per_mm: 1.0 / area_filament, layer_h: sliced.layer_height, retracted: false };
    w_.line("; generated by SolidCraft slice");
    w_.line("G21 ; millimetres");
    w_.line("G90 ; absolute moves");
    w_.line("M83 ; relative extrusion");
    let _ = writeln!(w_.text, "M140 S{:.0}", cfg.bed_temp);
    let _ = writeln!(w_.text, "M104 S{:.0}", cfg.nozzle_temp);
    let _ = writeln!(w_.text, "M190 S{:.0}", cfg.bed_temp);
    let _ = writeln!(w_.text, "M109 S{:.0}", cfg.nozzle_temp);
    let start = cfg.start_gcode.clone();
    w_.line(&start);
    let walls = cfg.walls.min(MAX_WALLS);
    for (n, layer) in sliced.layers.iter().enumerate() {
        let _ = writeln!(w_.text, ";LAYER:{n}");
        let _ = writeln!(w_.text, "G1 Z{:.3} F600", layer.top);
        let shift = |l: &[Vec2]| -> Vec<Vec2> { l.iter().map(|p| Vec2::new(p.x - centre.x, p.y - centre.y)).collect() };
        let mut inner: Vec<Vec<Vec2>> = Vec::new();
        for l in &layer.loops {
            let mut cur = shift(l);
            for k in 0..walls {
                let Some(wall) = offset(&cur, if k == 0 { cfg.line_width * 0.5 } else { cfg.line_width }) else { break };
                w_.path(&wall, true);
                cur = wall;
            }
            if let Some(core) = offset(&cur, cfg.line_width * 0.5) {
                inner.push(core);
            }
        }
        if cfg.infill_spacing > 0.0 && !inner.is_empty() {
            let angle = if n % 2 == 0 { std::f64::consts::FRAC_PI_4 } else { -std::f64::consts::FRAC_PI_4 };
            for (a, b) in infill(&inner, cfg.infill_spacing, angle) {
                w_.path(&[a, b], false);
            }
        }
    }
    let end = cfg.end_gcode.clone();
    w_.line("M104 S0");
    w_.line("M140 S0");
    w_.line(&end);
    Ok(w_.text)
}

/// Slices `tris` and writes G-code in one step.
pub fn print_triangles(tris: &[Tri], layer_height: f64, cfg: &PrintConfig) -> Result<String, String> {
    let sliced = slice(tris, layer_height).map_err(|e| e.to_string())?;
    gcode(&sliced, cfg).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An axis-aligned box from (0,0,0) to (w,d,h), 12 triangles.
    pub fn cuboid(w: f64, d: f64, h: f64, ox: f64) -> Vec<Tri> {
        let v = |x: f64, y: f64, z: f64| Vec3::new(ox + x * w, y * d, z * h);
        let q = |a: Vec3, b: Vec3, c: Vec3, e: Vec3| vec![[a, b, c], [a, c, e]];
        let mut t = Vec::new();
        t.extend(q(v(0., 0., 0.), v(1., 0., 0.), v(1., 1., 0.), v(0., 1., 0.)));
        t.extend(q(v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)));
        t.extend(q(v(0., 0., 0.), v(1., 0., 0.), v(1., 0., 1.), v(0., 0., 1.)));
        t.extend(q(v(1., 0., 0.), v(1., 1., 0.), v(1., 1., 1.), v(1., 0., 1.)));
        t.extend(q(v(1., 1., 0.), v(0., 1., 0.), v(0., 1., 1.), v(1., 1., 1.)));
        t.extend(q(v(0., 1., 0.), v(0., 0., 0.), v(0., 0., 1.), v(0., 1., 1.)));
        t
    }

    #[test]
    fn box_gives_one_ccw_square_per_layer() {
        let s = slice(&cuboid(10.0, 10.0, 2.0, 0.0), 0.2).unwrap();
        assert_eq!(s.layers.len(), 10);
        for l in &s.layers {
            assert_eq!(l.loops.len(), 1);
            assert!((area(&l.loops[0]) - 100.0).abs() < 1e-6);
        }
    }

    #[test]
    fn a_box_with_a_missing_side_is_reported_as_open() {
        let mut t = cuboid(10.0, 10.0, 2.0, 0.0);
        t.truncate(t.len() - 2);
        let s = slice(&t, 0.2).unwrap();
        assert_eq!(slice(&cuboid(10.0, 10.0, 2.0, 0.0), 0.2).unwrap().open_outlines(), 0);
        assert!(s.open_outlines() > 0);
    }

    #[test]
    fn two_separate_boxes_give_two_loops() {
        let mut t = cuboid(10.0, 10.0, 2.0, 0.0);
        t.extend(cuboid(10.0, 10.0, 2.0, 30.0));
        let s = slice(&t, 0.2).unwrap();
        assert!(s.layers.iter().all(|l| l.loops.len() == 2));
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert_eq!(slice(&[], 0.2), Err(SliceError::Empty));
        assert!(slice(&cuboid(1.0, 1.0, 1.0, 0.0), f64::NAN).is_err());
        assert!(slice(&cuboid(1.0, 1.0, 1.0, 0.0), 0.0).is_err());
        let nan = [[Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 1.0)]];
        assert!(slice(&nan, 0.2).is_err());
        assert!(slice(&cuboid(1.0, 1.0, 1000.0, 0.0), 1e-3 * 1.5).is_err());
    }

    #[test]
    fn offset_shrinks_a_square() {
        let sq = vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(10., 10.), Vec2::new(0., 10.)];
        let o = offset(&sq, 1.0).unwrap();
        assert!((area(&o) - 64.0).abs() < 1e-6);
        assert!(offset(&sq, 6.0).is_none());
    }

    #[test]
    fn infill_covers_a_square() {
        let sq = vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(10., 10.), Vec2::new(0., 10.)];
        let lines = infill(&[sq], 2.0, 0.0);
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|(a, b)| (b.x - a.x - 10.0).abs() < 1e-9));
    }

    #[test]
    fn gcode_has_layers_and_positive_extrusion() {
        let g = print_triangles(&cuboid(20.0, 20.0, 2.0, 0.0), 0.2, &PrintConfig::default()).unwrap();
        assert_eq!(g.matches(";LAYER:").count(), 10);
        assert!(g.contains("G1 X"));
        assert!(g.contains("M109 S220"));
        for l in g.lines().filter(|l| l.starts_with("G1 X")) {
            let e: f64 = l.split(" E").nth(1).and_then(|s| s.split(' ').next()).and_then(|s| s.parse().ok()).unwrap();
            assert!(e > 0.0);
        }
    }

    #[test]
    fn a_shape_bigger_than_the_bed_is_refused() {
        let r = print_triangles(&cuboid(500.0, 20.0, 2.0, 0.0), 0.2, &PrintConfig::default());
        assert!(r.unwrap_err().contains("does not fit"));
    }
}
