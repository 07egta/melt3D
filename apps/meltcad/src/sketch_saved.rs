//! Saved sketch geometry shared by the interactive tools and headless commands.
//! These helpers never open a window or mutate a scene.

use crate::curve;
use crate::curve_tool::{to_local, to_world};
use crate::model::{GuidePlane, Kind, MAX_OUTLINE, MAX_PATH, Material, Op, Shape};
use crate::sdf::{self, V3};
use crate::sketch::{self, Frame};

const LINE_RADIUS: f32 = 0.6;

/// The unit normal of a nearly planar loop, with a stable sign for legacy files.
pub(crate) fn loop_normal(pts: &[V3]) -> Option<V3> {
    if pts.len() < 3 || !pts.iter().all(|p| p.iter().all(|x| x.is_finite())) {
        return None;
    }
    let mut n = [0.0f32; 3];
    for (i, a) in pts.iter().enumerate() {
        let b = pts.get((i + 1) % pts.len())?;
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = sdf::dot(n, n).sqrt();
    if !l.is_finite() || l < 1e-6 {
        return None;
    }
    let n = sdf::mul(n, 1.0 / l);
    let up = if n[2].abs() >= 0.02 { n[2] >= 0.0 } else { n[1] < -1e-3 || (n[1].abs() <= 1e-3 && n[0] >= 0.0) };
    Some(if up { n } else { sdf::mul(n, -1.0) })
}

/// Save the already sampled vertices without smoothing or reducing them again.
pub(crate) fn create(frame: Frame, points: &[[f32; 2]], holes: &[Vec<[f32; 2]>], closed: bool) -> Result<Shape, String> {
    let cap = if closed { MAX_OUTLINE } else { MAX_PATH };
    if points.len() > cap {
        return Err(format!("A sketch can have up to {cap} points"));
    }
    let points = if closed {
        sketch::validate_profile(points).map_err(|e| e.message())?
    } else {
        if !holes.is_empty() {
            return Err("An open sketch path cannot have holes".into());
        }
        sketch::open_line(points, false).map_err(|e| e.message())?
    };
    let holes = if closed { sketch::validate_holes(&points, holes).map_err(|e| e.message())? } else { Vec::new() };
    if !frame.origin.iter().all(|v| v.is_finite() && v.abs() <= 5000.0) || Frame::from_axes(frame.origin, frame.u, frame.v).is_none() {
        return Err("Choose a finite, valid sketch plane".into());
    }
    let world: Vec<V3> = points.iter().map(|p| frame.to_world(*p)).collect();
    if !world.iter().all(|p| p.iter().all(|x| x.is_finite())) {
        return Err("Sketch coordinates must be finite".into());
    }
    let mut sh = Shape::new(Kind::Curve, frame.origin, Material { color: [0.98, 0.78, 0.3], ..Material::default() });
    sh.path = world.iter().map(|w| [w[0] - frame.origin[0], w[1] - frame.origin[1], w[2] - frame.origin[2], LINE_RADIUS]).collect();
    sh.params = [if closed { 1.0 } else { 0.0 }, 0.0, 0.0, 1.0];
    sh.name = if closed { "Sketch" } else { "Sketch path" }.into();
    sh.op = Op::Add;
    sh.blend = 0.0;
    sh.guide = true;
    sh.guide_plane = Some(GuidePlane { u: frame.u, v: frame.v, holes });
    Ok(sh)
}

/// The current plane includes the shape's translation, rotation and scale.
pub(crate) fn frame(sh: &Shape) -> Option<Frame> {
    if sh.kind != Kind::Curve || sh.path.len() < 2 || sh.path.len() > MAX_OUTLINE {
        return None;
    }
    if let Some(plane) = &sh.guide_plane {
        let origin = to_world(sh, [0.0; 3]);
        let u = sdf::sub(to_world(sh, plane.u), origin);
        let v = sdf::sub(to_world(sh, plane.v), origin);
        Frame::from_axes(origin, u, v)
    } else {
        let world: Vec<_> = sh.path.iter().map(|n| to_world(sh, [n[0], n[1], n[2]])).collect();
        Some(Frame::from_hit(sh.pos, loop_normal(&world)?))
    }
}

/// A closed curve's plane and vertices, using exactly the saved guide conversion.
pub(crate) fn profile(sh: &Shape) -> Option<(Frame, Vec<[f32; 2]>)> {
    if sh.kind != Kind::Curve || sh.path.len() < 3 || !curve::settings(&sh.params).0 {
        return None;
    }
    let f = frame(sh)?;
    let world: Vec<V3> = sh.path.iter().map(|n| to_world(sh, [n[0], n[1], n[2]])).collect();
    let tolerance = 0.001_f32.max(world.iter().map(|p| sdf::len(sdf::sub(*p, f.origin))).fold(0.0, f32::max) * 1e-5);
    if world.iter().any(|p| !p.iter().all(|v| v.is_finite()) || sdf::dot(sdf::sub(*p, f.origin), f.normal()).abs() > tolerance) {
        return None;
    }
    let flat: Vec<_> = world
        .iter()
        .map(|w| {
            let d = sdf::sub(*w, f.origin);
            [sdf::dot(d, f.u), sdf::dot(d, f.v)]
        })
        .collect();
    Some((f, sketch::validate_profile(&flat).ok()?))
}

/// Holes in the same current world plane as `profile`.
pub(crate) fn holes(sh: &Shape, frame: &Frame) -> Vec<Vec<[f32; 2]>> {
    let Some(plane) = &sh.guide_plane else { return Vec::new() };
    plane
        .holes
        .iter()
        .map(|hole| hole.iter().map(|p| frame.from_world(to_world(sh, sdf::add(sdf::mul(plane.u, p[0]), sdf::mul(plane.v, p[1]))))).collect())
        .collect()
}

/// Replace only a guide's geometry. Keep its identity, appearance and transforms.
/// New points are millimetres in the current drawing plane; an explicit plane relocates it.
pub(crate) fn replace(
    original: &Shape,
    plane: Option<Frame>,
    points: &[[f32; 2]],
    new_holes: Option<&[Vec<[f32; 2]>]>,
    closed: bool,
) -> Result<Shape, String> {
    if !original.guide || original.kind != Kind::Curve {
        return Err("Choose a saved sketch".into());
    }
    let old_frame = frame(original).ok_or("The saved sketch has no valid plane")?;
    let target = plane.unwrap_or(old_frame);
    let kept_holes = holes(original, &old_frame);
    let fresh = create(target, points, new_holes.unwrap_or(&kept_holes), closed)?;
    let mut result = original.clone();
    result.pos = target.origin;
    // Retain the existing transform while expressing the edited world points in local space.
    let local_plane = if plane.is_none() { original.guide_plane.as_ref().and_then(|p| Frame::from_axes([0.0; 3], p.u, p.v)) } else { None }
        .or_else(|| Frame::from_axes([0.0; 3], to_local(&result, target.to_world([1.0, 0.0])), to_local(&result, target.to_world([0.0, 1.0]))))
        .ok_or("The sketch transform cannot represent this plane")?;
    result.path = fresh
        .path
        .iter()
        .map(|p| {
            let q = to_local(&result, to_world(&fresh, [p[0], p[1], p[2]]));
            [q[0], q[1], q[2], LINE_RADIUS]
        })
        .collect();
    let local_holes: Vec<Vec<_>> = holes(&fresh, &target)
        .iter()
        .map(|hole| hole.iter().map(|p| local_plane.from_world(to_local(&result, target.to_world(*p)))).collect())
        .collect();
    if !result.path.iter().flatten().all(|v| v.is_finite() && v.abs() <= 5000.0)
        || !local_holes.iter().flatten().flatten().all(|v| v.is_finite() && v.abs() <= sketch::COORD_MAX)
    {
        return Err("Edited sketch exceeds the coordinate limits of its transform".into());
    }
    result.params = fresh.params;
    result.curve_handles.clear();
    result.guide_plane = if plane.is_none() && new_holes.is_none() && original.guide_plane.is_some() {
        original.guide_plane.clone()
    } else {
        Some(GuidePlane { u: local_plane.u, v: local_plane.v, holes: local_holes })
    };
    Ok(result)
}

/// A saved open path, in world coordinates, without resampling its vertices.
pub(crate) fn open_path(sh: &Shape) -> Result<Vec<V3>, String> {
    if sh.kind != Kind::Curve || curve::settings(&sh.params).0 {
        return Err("Choose an open sketch path".into());
    }
    if sh.path.len() > MAX_PATH {
        return Err(format!("A sweep path can have up to {MAX_PATH} points"));
    }
    let path: Vec<_> = sh.path.iter().map(|n| to_world(sh, [n[0], n[1], n[2]])).collect();
    if !path.iter().flatten().all(|v| v.is_finite() && v.abs() <= 5000.0) {
        return Err(sketch::SketchError::BadNumber.message());
    }
    if path.len() < 2 || path.windows(2).map(|w| sdf::len(sdf::sub(w[1], w[0]))).sum::<f32>() < 1.0 {
        return Err(sketch::SketchError::PathTooShort.message());
    }
    Ok(path)
}

/// Project a second saved sketch into the base axes, retaining actual spacing and offset.
pub(crate) fn loft_top(base: &Frame, bottom: &[[f32; 2]], top: &Shape) -> Result<(Vec<[f32; 2]>, f32, bool), String> {
    let (frame, profile) = profile(top).ok_or("The top sketch must be a closed, planar outline")?;
    if !holes(top, &frame).is_empty() {
        return Err("Loft needs an outline without holes".into());
    }
    let normal = base.normal();
    if sdf::dot(normal, frame.normal()).abs() < 0.9999 {
        return Err("Choose a top sketch on a parallel plane".into());
    }
    let height = sdf::dot(sdf::sub(frame.origin, base.origin), normal);
    if !height.is_finite() || !(0.1..=2000.0).contains(&height.abs()) {
        return Err("Place the second sketch 0.1–2000 mm from the first plane".into());
    }
    let top: Vec<_> = profile.iter().map(|p| base.from_world(frame.to_world(*p))).collect();
    sketch::loft(base, bottom, &top, height.abs(), height < 0.0).map_err(|e| e.message())?;
    Ok((top, height.abs(), height < 0.0))
}
