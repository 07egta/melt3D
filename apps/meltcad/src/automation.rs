//! Driving Melt 3D from a program or an AI: JSON commands on a scene.
//!
//! One command is one JSON object, `{"cmd": "shape.add", "kind": "Sphere", "pos": [0, 0, 20]}`, and the
//! answer is one JSON value (or an error text). The same commands run
//! - headless: `meltcad --run script.json [--scene in.meltcad] [--out out.meltcad] [--png out.png]`
//! - against the open window: `meltcad --control PORT` (JSON lines on 127.0.0.1, see `control.rs`).
//!
//! `execute` only touches a `Scene`, never a window, so every command is testable and nothing here
//! can open a dialog. Numbers that come in are hostile: they are checked and the scene is sanitised
//! afterwards. Nothing panics; a bad command returns `Err(text)`.

use serde_json::{Map, Value, json};

use crate::model::{Group, Kind, MAX_GROUPS, MAX_OUTLINE, MAX_SHAPES, Material, Scene, Shape};
use crate::sketch::{self, Frame};

/// The commands, for `commands` and for the error text of an unknown one.
pub const COMMANDS: &[(&str, &str)] = &[
    ("commands", "List the commands"),
    ("scene.get", "The whole scene as JSON"),
    ("scene.summary", "Short list of groups and shapes with their numbers"),
    ("scene.set", "Replace the scene: {scene: {...}}"),
    ("scene.new", "Empty scene (or {sphere: true} for the starting sphere)"),
    ("shape.add", "{kind, group?, name?, pos?, size?, rot?, scale?, color?, op?, blend?, round?, shell?, material?, outline?, params?}"),
    ("shape.set", "{group, shape, ...any shape field}: change a shape"),
    ("shape.delete", "{group, shape}"),
    ("group.add", "{name?, op?, blend?}"),
    ("group.set", "{group, name?, op?, blend?, visible?}"),
    ("group.delete", "{group}"),
    ("melt.set", "{amount?, drip?, ripple?, mode?}: how shapes melt together"),
    ("extrude", "{points: [[x,y],...], height, plane?: floor|front|side, z?, name?, op?, color?}: push an outline into a solid"),
    ("save", "{path}: write the scene file"),
    ("load", "{path}: read a scene file"),
    ("render", "{path, size?}: four views as a PNG (CPU picture)"),
];

/// Run one command on the scene. Returns the answer, or the reason it failed.
pub fn execute(scene: &mut Scene, cmd: &Value) -> Result<Value, String> {
    let obj = cmd.as_object().ok_or("A command is a JSON object: {\"cmd\": \"...\"}")?;
    let name = obj.get("cmd").and_then(Value::as_str).ok_or("Missing \"cmd\"")?;
    let out = match name {
        "commands" => Ok(json!(COMMANDS.iter().map(|(n, d)| json!({"cmd": n, "about": d})).collect::<Vec<_>>())),
        "scene.get" => serde_json::to_value(&*scene).map_err(|e| e.to_string()),
        "scene.summary" => Ok(summary(scene)),
        "scene.set" => {
            let v = obj.get("scene").ok_or("Missing \"scene\"")?;
            let text = serde_json::to_string(v).map_err(|e| e.to_string())?;
            *scene = Scene::from_json(&text)?;
            Ok(summary(scene))
        }
        "scene.new" => {
            *scene = if flag(obj, "sphere") { Scene::default() } else { Scene { groups: Vec::new(), ..Scene::default() } };
            Ok(summary(scene))
        }
        "shape.add" => shape_add(scene, obj),
        "shape.set" => shape_set(scene, obj),
        "shape.delete" => {
            let (g, s) = (index(obj, "group")?, index(obj, "shape")?);
            let group = scene.groups.get_mut(g).ok_or("No such group")?;
            if s >= group.shapes.len() {
                return Err("No such shape".into());
            }
            group.shapes.remove(s);
            Ok(json!({"deleted": [g, s]}))
        }
        "group.add" => {
            if scene.groups.len() >= MAX_GROUPS {
                return Err(format!("Up to {MAX_GROUPS} groups"));
            }
            let mut g = Group::new(format!("Group {}", scene.groups.len() + 1));
            patch_group(&mut g, obj)?;
            scene.groups.push(g);
            Ok(json!({"group": scene.groups.len() - 1}))
        }
        "group.set" => {
            let g = index(obj, "group")?;
            let group = scene.groups.get_mut(g).ok_or("No such group")?;
            patch_group(group, obj)?;
            Ok(json!({"group": g}))
        }
        "group.delete" => {
            let g = index(obj, "group")?;
            if g >= scene.groups.len() {
                return Err("No such group".into());
            }
            scene.groups.remove(g);
            Ok(json!({"deleted": g}))
        }
        "melt.set" => {
            let mut v = serde_json::to_value(scene.melt).map_err(|e| e.to_string())?;
            merge(&mut v, &Value::Object(without(obj, &["cmd", "token"])));
            scene.melt = serde_json::from_value(v).map_err(|e| format!("Bad melt settings: {e}"))?;
            Ok(json!({"amount": scene.melt.amount, "drip": scene.melt.drip, "ripple": scene.melt.ripple}))
        }
        "extrude" => extrude(scene, obj),
        "save" => {
            let path = text(obj, "path")?;
            std::fs::write(&path, scene.try_to_json()?).map_err(|e| format!("Cannot write {path}: {e}"))?;
            Ok(json!({"saved": path}))
        }
        "load" => {
            let path = text(obj, "path")?;
            let t = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {path}: {e}"))?;
            *scene = Scene::from_json(&t)?;
            Ok(summary(scene))
        }
        "render" => {
            let path = text(obj, "path")?;
            let size = obj.get("size").and_then(Value::as_u64).unwrap_or(220).clamp(40, 600) as usize;
            let (w, rgba) = crate::cpu_render::sheet(scene, size);
            let bytes = crate::png::encode_rgba(w as u32, w as u32, &rgba)?;
            std::fs::write(&path, bytes).map_err(|e| format!("Cannot write {path}: {e}"))?;
            Ok(json!({"png": path, "pixels": w}))
        }
        other => Err(format!("Unknown command \"{other}\". Try {{\"cmd\":\"commands\"}}")),
    };
    // whatever a command did, the scene stays inside the limits every other path keeps
    scene.sanitize();
    out
}

fn summary(scene: &Scene) -> Value {
    let groups: Vec<Value> = scene
        .groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let shapes: Vec<Value> = g
                .shapes
                .iter()
                .enumerate()
                .map(|(si, s)| json!({"shape": si, "name": s.name, "kind": format!("{:?}", s.kind), "op": format!("{:?}", s.op), "pos": s.pos, "size": s.size}))
                .collect();
            json!({"group": gi, "name": g.name, "op": format!("{:?}", g.op), "blend": g.blend, "visible": g.visible, "shapes": shapes})
        })
        .collect();
    json!({"groups": groups, "shape_count": scene.shape_count(), "melt": {"amount": scene.melt.amount}})
}

fn flag(obj: &Map<String, Value>, key: &str) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    obj.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| format!("Missing \"{key}\""))
}

fn index(obj: &Map<String, Value>, key: &str) -> Result<usize, String> {
    obj.get(key).and_then(Value::as_u64).map(|n| n as usize).ok_or_else(|| format!("Missing \"{key}\" (a number)"))
}

fn without(obj: &Map<String, Value>, skip: &[&str]) -> Map<String, Value> {
    obj.iter().filter(|(k, _)| !skip.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Deep merge: objects merge key by key, everything else is replaced.
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// The shape fields of a command (`color` is the short way to say `material.color`).
fn shape_patch(obj: &Map<String, Value>) -> Value {
    let mut p = without(obj, &["cmd", "token", "group", "shape", "color", "kind"]);
    if let Some(c) = obj.get("color") {
        let m = p.entry("material").or_insert_with(|| json!({}));
        if let Some(m) = m.as_object_mut() {
            m.insert("color".into(), c.clone());
        }
    }
    Value::Object(p)
}

fn shape_add(scene: &mut Scene, obj: &Map<String, Value>) -> Result<Value, String> {
    if scene.shape_count() >= MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    let kind: Kind = serde_json::from_value(obj.get("kind").cloned().ok_or("Missing \"kind\" (Sphere, Box, Cylinder, ...)")?)
        .map_err(|_| "Unknown kind. Examples: Sphere, Box, Cylinder, Torus, Cone, Capsule, Hemi, Tube, Wedge, Star".to_string())?;
    let mut shape = Shape::new(kind, [0.0, 0.0, 20.0], Material::default());
    let mut v = serde_json::to_value(&shape).map_err(|e| e.to_string())?;
    merge(&mut v, &shape_patch(obj));
    shape = serde_json::from_value(v).map_err(|e| format!("Bad shape field: {e}"))?;
    if scene.groups.is_empty() {
        scene.groups.push(Group::new("Group 1"));
    }
    let g = match obj.get("group") {
        Some(_) => index(obj, "group")?,
        None => scene.groups.len() - 1,
    };
    let group = scene.groups.get_mut(g).ok_or("No such group")?;
    group.shapes.push(shape);
    Ok(json!({"group": g, "shape": group.shapes.len() - 1}))
}

fn shape_set(scene: &mut Scene, obj: &Map<String, Value>) -> Result<Value, String> {
    let (g, s) = (index(obj, "group")?, index(obj, "shape")?);
    let shape = scene.shape_mut(g, s).ok_or("No such shape")?;
    let mut v = serde_json::to_value(&*shape).map_err(|e| e.to_string())?;
    let mut patch = shape_patch(obj);
    if let Some(k) = obj.get("kind") {
        merge(&mut patch, &json!({"kind": k}));
    }
    merge(&mut v, &patch);
    *shape = serde_json::from_value(v).map_err(|e| format!("Bad shape field: {e}"))?;
    Ok(json!({"group": g, "shape": s}))
}

fn patch_group(g: &mut Group, obj: &Map<String, Value>) -> Result<(), String> {
    let mut v = serde_json::to_value(&*g).map_err(|e| e.to_string())?;
    let mut p = Map::new();
    for key in ["name", "op", "blend", "visible"] {
        if let Some(x) = obj.get(key) {
            p.insert(key.into(), x.clone());
        }
    }
    merge(&mut v, &Value::Object(p));
    *g = serde_json::from_value(v).map_err(|e| format!("Bad group field: {e}"))?;
    Ok(())
}

fn extrude(scene: &mut Scene, obj: &Map<String, Value>) -> Result<Value, String> {
    if scene.shape_count() >= MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    let pts = obj.get("points").and_then(Value::as_array).ok_or("Missing \"points\": [[x, y], ...]")?;
    let mut profile: Vec<[f32; 2]> = Vec::new();
    for p in pts.iter().take(MAX_OUTLINE + 1) {
        let a = p.as_array().filter(|a| a.len() == 2).ok_or("Each point is [x, y]")?;
        let (x, y) = (a[0].as_f64().ok_or("Point numbers")? as f32, a[1].as_f64().ok_or("Point numbers")? as f32);
        if !x.is_finite() || !y.is_finite() {
            return Err("Point numbers must be finite".into());
        }
        profile.push([x, y]);
    }
    let height = obj.get("height").and_then(Value::as_f64).unwrap_or(20.0) as f32;
    let frame = match obj.get("plane").and_then(Value::as_str).unwrap_or("floor") {
        "floor" => match obj.get("z").and_then(Value::as_f64) {
            Some(z) if z.is_finite() => Frame::horizontal(z as f32),
            _ => Frame::floor(),
        },
        "front" => Frame::front(),
        "side" => Frame::side(),
        other => return Err(format!("Unknown plane \"{other}\" (floor, front, side)")),
    };
    let built = sketch::extrude(&frame, &profile, height, false).map_err(|e| e.message())?;
    let mut shape = Shape::new(built.kind, built.pos, Material::default());
    sketch::apply(&built, &mut shape);
    let mut v = serde_json::to_value(&shape).map_err(|e| e.to_string())?;
    merge(&mut v, &shape_patch(&without(obj, &["points", "height", "plane", "z"])));
    shape = serde_json::from_value(v).map_err(|e| format!("Bad shape field: {e}"))?;
    if scene.groups.is_empty() {
        scene.groups.push(Group::new("Group 1"));
    }
    let g = match obj.get("group") {
        Some(_) => index(obj, "group")?,
        None => scene.groups.len() - 1,
    };
    let group = scene.groups.get_mut(g).ok_or("No such group")?;
    group.shapes.push(shape);
    Ok(json!({"group": g, "shape": group.shapes.len() - 1}))
}

/// `meltcad --run script.json [--scene in] [--out out.meltcad] [--png out.png]`: true when it ran
/// (the caller then exits). The script is a JSON array of commands, or `{"commands": [...]}`.
pub fn cli(args: &[std::ffi::OsString]) -> bool {
    if args.get(1).and_then(|a| a.to_str()) != Some("--run") {
        return false;
    }
    let arg = |flag: &str| -> Option<String> {
        args.iter().position(|a| a.to_str() == Some(flag)).and_then(|i| args.get(i + 1)).map(|a| a.to_string_lossy().into_owned())
    };
    let Some(script) = args.get(2).map(|a| a.to_string_lossy().into_owned()) else {
        eprintln!("usage: meltcad --run script.json [--scene in.meltcad] [--out out.meltcad] [--png out.png]");
        return true;
    };
    let mut scene = match arg("--scene") {
        Some(p) => match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| Scene::from_json(&t)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read the scene: {e}");
                return true;
            }
        },
        None => Scene { groups: Vec::new(), ..Scene::default() },
    };
    let parsed: Result<Value, String> =
        std::fs::read_to_string(&script).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
    let cmds: Vec<Value> = match parsed {
        Ok(Value::Array(a)) => a,
        Ok(Value::Object(mut o)) => match o.remove("commands") {
            Some(Value::Array(a)) => a,
            _ => vec![Value::Object(o)],
        },
        Ok(_) => {
            eprintln!("the script must be a JSON array of commands");
            return true;
        }
        Err(e) => {
            eprintln!("cannot read the script: {e}");
            return true;
        }
    };
    let _guard = crate::mesh::LoadGuard::begin();
    for c in &cmds {
        match execute(&mut scene, c) {
            Ok(v) => println!("{v}"),
            Err(e) => println!("{}", json!({"error": e})),
        }
    }
    if let Some(p) = arg("--out") {
        match scene.try_to_json().and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string())) {
            Ok(()) => {}
            Err(e) => eprintln!("cannot write {p}: {e}"),
        }
    }
    if let Some(p) = arg("--png")
        && let Err(e) = execute(&mut scene, &json!({"cmd": "render", "path": p}))
    {
        eprintln!("cannot render: {e}");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(scene: &mut Scene, v: Value) -> Result<Value, String> {
        execute(scene, &v)
    }

    #[test]
    fn a_shape_is_added_changed_and_deleted_by_commands() {
        let mut sc = Scene { groups: Vec::new(), ..Scene::default() };
        let r = run(&mut sc, json!({"cmd": "shape.add", "kind": "Box", "pos": [5, 6, 7], "color": [1, 0, 0], "op": "Add"}));
        assert_eq!(r, Ok(json!({"group": 0, "shape": 0})));
        let s = sc.shape(0, 0).map(|s| (s.pos, s.material.color));
        assert_eq!(s, Some(([5.0, 6.0, 7.0], [1.0, 0.0, 0.0])));
        run(&mut sc, json!({"cmd": "shape.set", "group": 0, "shape": 0, "size": [30, 30, 30], "color": [0, 1, 0]})).ok();
        assert_eq!(sc.shape(0, 0).map(|s| s.size[0]), Some(30.0));
        assert_eq!(sc.shape(0, 0).map(|s| s.material.color), Some([0.0, 1.0, 0.0]));
        assert!(run(&mut sc, json!({"cmd": "shape.delete", "group": 0, "shape": 0})).is_ok());
        assert_eq!(sc.shape_count(), 0);
    }

    #[test]
    fn extrude_makes_a_solid_from_points() {
        let mut sc = Scene::default();
        let r = run(&mut sc, json!({"cmd": "extrude", "points": [[0, 0], [30, 0], [30, 20], [0, 20]], "height": 15}));
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(sc.shape_count(), 2);
    }

    #[test]
    fn hostile_commands_return_errors_and_never_panic() {
        let mut sc = Scene::default();
        for bad in [
            json!(5),
            json!({}),
            json!({"cmd": 3}),
            json!({"cmd": "nope"}),
            json!({"cmd": "shape.add"}),
            json!({"cmd": "shape.add", "kind": "Banana"}),
            json!({"cmd": "shape.add", "kind": "Box", "size": "big"}),
            json!({"cmd": "shape.set", "group": 99, "shape": 0}),
            json!({"cmd": "shape.delete", "group": -1, "shape": 0}),
            json!({"cmd": "extrude", "points": [[0, 0], [1]]}),
            json!({"cmd": "extrude", "points": [[0, 0], [1, 1], [2, 0]], "plane": "moon"}),
            json!({"cmd": "melt.set", "amount": "lots"}),
            json!({"cmd": "save"}),
        ] {
            assert!(run(&mut sc, bad.clone()).is_err(), "{bad}");
        }
        // huge numbers are tamed by the sanitiser, not rejected or crashed on
        let _ = run(&mut sc, json!({"cmd": "shape.add", "kind": "Sphere", "size": [1e30, 1e30, 1e30], "pos": [1e30, 0, 0]}));
        assert!(sc.groups.iter().flat_map(|g| &g.shapes).all(|s| s.size.iter().all(|v| v.is_finite() && *v < 1e6)));
        // limits hold
        for _ in 0..(MAX_SHAPES + 5) {
            let _ = run(&mut sc, json!({"cmd": "shape.add", "kind": "Sphere"}));
        }
        assert!(sc.shape_count() <= MAX_SHAPES);
    }

    #[test]
    fn save_load_and_render_round_trip() {
        let dir = std::env::temp_dir().join(format!("meltcad-automation-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (f, p) = (dir.join("a.meltcad"), dir.join("a.png"));
        let mut sc = Scene::default();
        assert!(run(&mut sc, json!({"cmd": "save", "path": f.to_string_lossy()})).is_ok());
        let mut back = Scene { groups: Vec::new(), ..Scene::default() };
        assert!(run(&mut back, json!({"cmd": "load", "path": f.to_string_lossy()})).is_ok());
        assert_eq!(back.shape_count(), sc.shape_count());
        assert!(run(&mut back, json!({"cmd": "render", "path": p.to_string_lossy(), "size": 40})).is_ok());
        assert!(std::fs::metadata(&p).is_ok_and(|m| m.len() > 100));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
