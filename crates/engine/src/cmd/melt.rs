//! Melt: melt solids together (or scoop one out of another) with a soft, flowing blend.
//!
//! The result is a mesh body baked into the timeline: it suits 3D printing and organic shapes,
//! not further feature work on exact faces.

use serde_json::{Value, json};
use solidcraft_doc::{FeatureKind, MeshData};
use solidcraft_geom::melt::{self, TriMesh};

use super::CommandSpec;
use crate::{
    EngineError, Result, Session,
    params::{bad, str_, string_list},
};

pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new("solid.melt", "Melt", melt_cmd)
    .at("SOLID", "MODIFY")
    .icon("fillet")
    .params("bodies: [names] (two or more); strength?: blend reach in mm (default 5; 0 = sharp); mode?: join | cut (cut removes the later bodies from the first; default join); resolution?: grid points along the longest side, 16–256 (default 96); smooth?: smoothing rounds 0–8 (default 2); keep?: keep the original bodies (default false); name? → a baked mesh body `Melt`")];

fn melt_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "solid.melt";
    let names = string_list(p, "bodies");
    if names.len() < 2 {
        return Err(bad(cmd, "`bodies` must list at least two bodies"));
    }
    let mode = str_(p, "mode").unwrap_or("join");
    let cut = match mode {
        "join" => false,
        "cut" => true,
        other => return Err(bad(cmd, format!("`mode` must be join or cut, not `{other}`"))),
    };
    let strength = match p.get("strength") {
        None | Some(Value::Null) => 5.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "`strength` must be a number"))?,
    };
    if !(0.0..=1000.0).contains(&strength) {
        return Err(bad(cmd, "`strength` must be between 0 and 1000 mm"));
    }
    let resolution = match p.get("resolution") {
        None | Some(Value::Null) => 96.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "`resolution` must be a number"))?,
    };
    if !(16.0..=256.0).contains(&resolution) {
        return Err(bad(cmd, "`resolution` must be between 16 and 256"));
    }
    let smooth = match p.get("smooth") {
        None | Some(Value::Null) => 2.0,
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "`smooth` must be a number"))?,
    };
    if !(0.0..=8.0).contains(&smooth) {
        return Err(bad(cmd, "`smooth` must be between 0 and 8"));
    }
    let keep = p.get("keep").and_then(Value::as_bool).unwrap_or(false);

    let st = s.model.state();
    let mut meshes = Vec::with_capacity(names.len());
    for n in &names {
        let b = st.body(n).ok_or_else(|| bad(cmd, format!("no body `{n}`")))?;
        let m = b.body.tessellate((b.body.size() * 2e-3).max(1e-3)).map_err(|e| bad(cmd, format!("`{n}`: {e}")))?;
        meshes.push(TriMesh { positions: m.positions.iter().map(|v| [v.x, v.y, v.z]).collect(), triangles: m.triangles.clone() });
    }
    let params = melt::Params { strength, resolution: resolution as usize, cut, smooth: smooth as usize };
    let out = melt::melt(&meshes, &params).map_err(|e| bad(cmd, e))?;
    let volume = melt::volume(&out);
    let data = MeshData {
        name: "Melt".into(),
        positions: out.positions.iter().flat_map(|v| v.iter().copied()).collect(),
        triangles: out.triangles.iter().flat_map(|t| t.iter().copied()).collect(),
        color: None,
    };
    let fname = str_(p, "name").filter(|n| !n.trim().is_empty()).unwrap_or("Melt").to_string();
    let id = s.doc_mut().add_feature(FeatureKind::MeshImport { file: "Melt".into(), meshes: vec![data] }, Some(&fname))?;
    s.active_sketch = None;
    s.refresh();
    if let Some(e) = s.model.result(id).and_then(|r| r.error.clone()) {
        return Err(EngineError::Other(e));
    }
    let created: Vec<String> = s.model.state().bodies.iter().filter(|b| b.feature == id).map(|b| b.name.clone()).collect();
    if !keep {
        let spec = super::find_command("solid.remove").ok_or_else(|| EngineError::UnknownCommand("solid.remove".into()))?;
        spec.run_nested(s, &json!({ "bodies": names }))?;
        s.refresh();
    }
    Ok(json!({"feature": id, "created_bodies": created, "volume": volume, "triangles": out.triangles.len(), "mode": mode, "strength": strength}))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Session;

    fn run(s: &mut Session, id: &str, p: Value) -> Value {
        match s.execute(id, &p) {
            Ok(v) => v,
            Err(e) => panic!("{id} {p}: {e}"),
        }
    }

    fn two_blocks() -> Session {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 20, "width": 20, "height": 20, "corner": [0, 0, 0], "body_name": "A"}));
        run(&mut s, "solid.box", json!({"length": 20, "width": 20, "height": 20, "corner": [30, 0, 0], "operation": "new", "body_name": "B"}));
        s
    }

    #[test]
    fn melting_two_separate_blocks_bridges_them_into_one_body() {
        let mut s = two_blocks();
        let r = run(&mut s, "solid.melt", json!({"bodies": ["A", "B"], "strength": 8, "resolution": 64}));
        assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(1));
        let st = s.model.state();
        assert_eq!(st.bodies.len(), 1, "the originals are removed");
        assert!(r["volume"].as_f64().unwrap_or(0.0) > 2.0 * 8000.0 * 0.9, "the bridge adds material: {r}");
    }

    #[test]
    fn keep_leaves_the_originals() {
        let mut s = two_blocks();
        run(&mut s, "solid.melt", json!({"bodies": ["A", "B"], "strength": 4, "resolution": 48, "keep": true}));
        assert_eq!(s.model.state().bodies.len(), 3);
    }

    #[test]
    fn hostile_params_are_errors() {
        let mut s = two_blocks();
        for p in [
            json!({"bodies": ["A"]}),
            json!({"bodies": ["A", "Nope"]}),
            json!({"bodies": ["A", "B"], "strength": -1}),
            json!({"bodies": ["A", "B"], "strength": 1e300}),
            json!({"bodies": ["A", "B"], "resolution": 100000}),
            json!({"bodies": ["A", "B"], "mode": "weird"}),
        ] {
            assert!(s.execute("solid.melt", &p).is_err(), "{p}");
        }
        assert_eq!(s.model.state().bodies.len(), 2);
    }
}
