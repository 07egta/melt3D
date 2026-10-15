//! Surface analyses: zebra stripes, draft angle and curvature map shading of the model's faces.
//! View state only: no geometry changes and no undo step.

use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use super::CommandSpec;
use crate::params::{bad, bool_, num, vec3};
use crate::{Result, Session, SurfaceAnalysis};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("inspect.zebra", "Zebra Analysis", zebra)
        .at("SKETCH", "INSPECT")
        .icon("zebra")
        .noundo()
        .params("stripes?: number across the view (default 12); clear?: true turns the analysis off"),
    CommandSpec::new("inspect.draft", "Draft Analysis", draft)
        .at("SKETCH", "INSPECT")
        .icon("draft_analysis")
        .noundo()
        .params("pull?: [x,y,z] direction (default Z); angle?: degrees (default 1); clear?: true turns the analysis off"),
    CommandSpec::new("inspect.environment_map", "Environment Map Analysis", |s, p| set(s, p, SurfaceAnalysis::Environment))
        .at("SKETCH", "INSPECT")
        .icon("environment_map")
        .noundo()
        .params("clear?: true turns the analysis off"),
    CommandSpec::new("inspect.accessibility", "Accessibility Analysis", accessibility)
        .at("SKETCH", "INSPECT")
        .icon("accessibility")
        .noundo()
        .params("direction?: [x,y,z] the tool comes from (default Z, from above); clear?: true turns the analysis off"),
    CommandSpec::new("inspect.curvature_map", "Curvature Map Analysis", curvature_map)
        .at("SKETCH", "INSPECT")
        .icon("curvature_map")
        .noundo()
        .params("radius?: mm shown red (default: a tenth of the model's size); clear?: true turns the analysis off"),
    CommandSpec::new("inspect.continuity", "Continuity Analysis", continuity)
        .at("SURFACE", "INSPECT")
        .icon("continuity")
        .noundo()
        .params("a, b: {body, edge: [x,y,z]} (a point on a free edge of each surface; the edges are expected to lie together). Reads how the two surfaces meet along the edge, at 39 points, and reports the worst: gap_mm (G0), angle_deg between the normals (G1), curvature_jump and curvature_max (G2, in 1/mm), g3_jump (an estimate of the jump in the curvature's rate of change, 1/mm²; noisy), and level: the highest of G0, G1, G2 met (none when the surfaces do not even touch). Limits: gap 1e-4 of the size, 0.5 degrees, a tenth of the larger curvature. G3 and Class A are not decided by this check"),
];

fn set(s: &mut Session, p: &Value, a: SurfaceAnalysis) -> Result<Value> {
    if bool_(p, "clear").unwrap_or(false) {
        s.analysis = None;
    } else {
        s.analysis = Some(a);
    }
    s.revision += 1;
    Ok(json!({ "analysis": s.analysis }))
}

fn zebra(s: &mut Session, p: &Value) -> Result<Value> {
    let stripes = num(p, "stripes").unwrap_or(12.0);
    if !(stripes.is_finite() && (1.0..=200.0).contains(&stripes)) {
        return Err(bad("inspect.zebra", "`stripes` must be 1…200"));
    }
    set(s, p, SurfaceAnalysis::Zebra { stripes })
}

fn draft(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "inspect.draft";
    let pull = match p.get("pull") {
        Some(v) => vec3(v).and_then(Vec3::normalized).ok_or_else(|| bad(cmd, "`pull` must be a non-zero [x,y,z]"))?,
        None => Vec3::Z,
    };
    let angle = num(p, "angle").unwrap_or(1.0);
    if !(angle.is_finite() && (0.0..90.0).contains(&angle)) {
        return Err(bad(cmd, "`angle` must be 0…90 degrees"));
    }
    set(s, p, SurfaceAnalysis::Draft { pull, angle })
}

fn accessibility(s: &mut Session, p: &Value) -> Result<Value> {
    let dir = match p.get("direction") {
        Some(v) => vec3(v).and_then(Vec3::normalized).ok_or_else(|| bad("inspect.accessibility", "`direction` must be a non-zero [x,y,z]"))?,
        None => Vec3::Z,
    };
    set(s, p, SurfaceAnalysis::Access { dir })
}

fn curvature_map(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "inspect.curvature_map";
    let radius = match num(p, "radius") {
        Some(r) if r.is_finite() && r > 1e-6 && r < 1e9 => r,
        Some(_) => return Err(bad(cmd, "`radius` must be positive")),
        None => {
            let st = s.model.state();
            let size = st.bodies.iter().map(|b| b.mesh().bounds().diagonal()).fold(0.0_f64, f64::max);
            if size > 0.0 { size / 10.0 } else { 10.0 }
        }
    };
    set(s, p, SurfaceAnalysis::Curvature { radius })
}

fn continuity(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "inspect.continuity";
    let end = |key: &str| -> Result<(String, Vec3)> {
        let side = p.get(key).ok_or_else(|| bad(cmd, format!("`{key}` is needed: {{body, edge: [x,y,z]}}")))?;
        let body =
            side.get("body").and_then(Value::as_str).filter(|b| !b.trim().is_empty()).ok_or_else(|| bad(cmd, format!("`{key}.body` is needed")))?;
        let at = side
            .get("edge")
            .and_then(vec3)
            .or_else(|| side.get("edges").and_then(Value::as_array).and_then(|a| a.first()).and_then(vec3))
            .ok_or_else(|| bad(cmd, format!("`{key}.edge` must be a point [x,y,z] on a free edge")))?;
        Ok((body.to_string(), at))
    };
    let (ba, pa) = end("a")?;
    let (bb, pb) = end("b")?;
    let st = s.model.state();
    let a = st.body(&ba).ok_or_else(|| bad(cmd, format!("no body `{ba}`")))?;
    let b = st.body(&bb).ok_or_else(|| bad(cmd, format!("no body `{bb}`")))?;
    let c = solidcraft_kernel::continuity(&a.body, pa, &b.body, pb).map_err(|e| bad(cmd, e.to_string()))?;
    let size = a.body.size().max(b.body.size());
    let level = match c.level(size) {
        None => "none",
        Some(0) => "G0",
        Some(1) => "G1",
        _ => "G2",
    };
    Ok(json!({
        "level": level,
        "samples": c.samples,
        "gap_mm": c.gap,
        "angle_deg": c.angle.to_degrees(),
        "curvature_jump": c.curvature_jump,
        "curvature_max": c.curvature_max,
        "g3_jump": c.g3_jump,
        "note": "G3 and Class A are not decided by this check; g3_jump is an estimate"
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Session, SurfaceAnalysis};

    #[test]
    fn surface_analyses_are_view_state() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length": 10, "width": 10, "height": 10})).unwrap();
        let undo = s.undo.len();
        s.execute("inspect.zebra", &json!({"stripes": 20})).unwrap();
        assert_eq!(s.analysis, Some(SurfaceAnalysis::Zebra { stripes: 20.0 }));
        s.execute("inspect.draft", &json!({"pull": [0, 0, 2], "angle": 3})).unwrap();
        assert!(matches!(s.analysis, Some(SurfaceAnalysis::Draft { angle, .. }) if angle == 3.0));
        s.execute("inspect.curvature_map", &json!({})).unwrap();
        assert!(matches!(s.analysis, Some(SurfaceAnalysis::Curvature { radius }) if (radius - 300f64.sqrt() / 10.0).abs() < 1e-6));
        s.execute("inspect.environment_map", &json!({})).unwrap();
        assert_eq!(s.analysis, Some(SurfaceAnalysis::Environment));
        s.execute("inspect.accessibility", &json!({"direction": [0, 0, -3]})).unwrap();
        assert_eq!(s.analysis, Some(SurfaceAnalysis::Access { dir: solidcraft_geom::Vec3::new(0.0, 0.0, -1.0) }));
        s.execute("inspect.curvature_map", &json!({"clear": true})).unwrap();
        assert_eq!(s.analysis, None);
        assert_eq!(s.undo.len(), undo, "no undo steps");
        assert!(s.execute("inspect.zebra", &json!({"stripes": 0})).is_err());
        assert!(s.execute("inspect.draft", &json!({"pull": [0, 0, 0]})).is_err());
    }
}
