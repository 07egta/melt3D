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

use crate::model::{Group, Kind, MAX_GROUPS, MAX_OUTLINE, MAX_PATH, MAX_SHAPES, Material, Op, Scene, Shape};
use crate::sketch::{self, Frame};
use crate::sketch_saved;

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
    ("sketch.create", "{points, closed?:true, holes?:[], plane?:floor|front|side|{origin,u,v}, group?, name?}: save a finished sketch"),
    ("sketch.set", "{group, shape, points, closed?, holes?, plane?, name?}: replace saved sketch geometry; omitted metadata stays"),
    ("solid.extrude", "{sketch:{group,shape}, height?:20, both?:false, flip?:false, op?:Add|Subtract, name?, color?}"),
    ("solid.revolve", "{sketch:{group,shape}, op?:Add|Subtract, name?, color?}: revolve a saved standing profile"),
    ("solid.loft", "{sketch, top?:{group,shape}, height?:20, top_scale?:0.5, flip?, op?, name?, color?}: saved top or scaled copy"),
    ("solid.sweep", "{sketch, path?:{group,shape}, scale?:1, radius?:4, op?, name?, color?}: closed profile + path, or open sketch + radius"),
    ("anim.get", "The animation: fps, length, clay, poses (frames that hold a pose)"),
    ("anim.set", "{fps?, length?, clay?}: speed, number of frames, hold each pose 3 frames (stop motion look)"),
    (
        "anim.key",
        "{frame}: store the scene as it is now as the pose at this frame. Pose, move things, store again: the frames between are made for you (melt too)",
    ),
    ("anim.delete", "{frame}: forget the pose at this frame"),
    ("anim.clear", "Forget every pose"),
    ("anim.at", "{frame}: put the scene into its pose at this frame; returns the frame clamped to the animation length"),
    ("anim.render", "{path, size?, gpu?}: the animation as a GIF (gpu: true draws it like the window does, when there is a graphics card)"),
    ("save", "{path}: write the scene file"),
    ("load", "{path}: read a scene file"),
    ("scene.add_file", "{path}: add another scene file to this one, in a new group (a piece of the big file) that melts with what is there"),
    ("piece.reload", "{group}: read the file of a piece again and replace the piece with it"),
    ("piece.save", "{group, path}: write a piece into a file of its own (the piece remembers it)"),
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
        // Build and validate before committing. Errors return before sanitisation can
        // even assign IDs to an otherwise unchanged input scene.
        "sketch.create" => Ok(sketch_save(scene, obj, false)?),
        "sketch.set" => Ok(sketch_save(scene, obj, true)?),
        "solid.extrude" | "solid.revolve" | "solid.loft" | "solid.sweep" => Ok(solid_from_sketch(scene, obj, name)?),
        "anim.get" => Ok(json!({"fps": scene.anim.fps, "length": scene.anim.length, "clay": scene.anim.clay, "poses": scene.anim.key_frames()})),
        "anim.set" => {
            if let Some(v) = obj.get("fps") {
                scene.anim.fps = v.as_u64().ok_or("fps is a number")?.min(60) as u32;
            }
            if let Some(v) = obj.get("length") {
                scene.anim.length = v.as_u64().ok_or("length is a number")?.min(u64::from(crate::anim::MAX_FRAMES)) as u32;
            }
            if let Some(v) = obj.get("clay") {
                scene.anim.clay = v.as_bool().ok_or("clay is true or false")?;
            }
            scene.anim.sanitize();
            Ok(json!({"fps": scene.anim.fps, "length": scene.anim.length, "clay": scene.anim.clay}))
        }
        "anim.key" => {
            let f = frame_arg(obj)?;
            let mut anim = std::mem::take(&mut scene.anim);
            let r = anim.capture(scene, f);
            scene.anim = anim;
            r?;
            Ok(json!({"poses": scene.anim.key_frames()}))
        }
        "anim.delete" => {
            scene.anim.remove(frame_arg(obj)?);
            Ok(json!({"poses": scene.anim.key_frames()}))
        }
        "anim.clear" => {
            scene.anim.clear();
            Ok(json!({"poses": []}))
        }
        "anim.at" => {
            let anim = scene.anim.clone();
            let frame = frame_arg(obj)?.min(anim.length.saturating_sub(1));
            anim.apply(scene, frame);
            let mut reply = summary(scene);
            reply["frame"] = json!(frame);
            Ok(reply)
        }
        "anim.render" => {
            let path = text(obj, "path")?;
            let size = obj.get("size").and_then(Value::as_u64).unwrap_or(240) as usize;
            let bytes = crate::cpu_render::animation_gif(scene, None, size, flag(obj, "gpu"), |_, _| true)?;
            std::fs::write(&path, bytes).map_err(|e| format!("Cannot write {path}: {e}"))?;
            Ok(json!({"gif": path}))
        }
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
        "piece.reload" => {
            let g = index(obj, "group")?;
            crate::add_file::reload_piece(scene, g)?;
            Ok(summary(scene))
        }
        "piece.save" => {
            let g = index(obj, "group")?;
            let path = text(obj, "path")?;
            crate::add_file::save_piece(scene, g, std::path::Path::new(&path))?;
            Ok(json!({"saved": path}))
        }
        "scene.add_file" => {
            let path = text(obj, "path")?;
            crate::add_file::add_file(scene, std::path::Path::new(&path))?;
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
                .map(|(si, s)| json!({"shape": si, "name": s.name, "kind": format!("{:?}", s.kind), "op": format!("{:?}", s.op), "pos": s.pos, "size": s.size, "sketch": s.guide}))
                .collect();
            json!({"group": gi, "name": g.name, "op": format!("{:?}", g.op), "blend": g.blend, "visible": g.visible, "shapes": shapes})
        })
        .collect();
    json!({"groups": groups, "shape_count": scene.shape_count(), "melt": {"amount": scene.melt.amount}})
}

fn frame_arg(obj: &Map<String, Value>) -> Result<u32, String> {
    let n = obj.get("frame").and_then(Value::as_u64).ok_or("Missing \"frame\" (a whole number)")?;
    u32::try_from(n).map_err(|_| "frame is too big".to_string())
}

fn flag(obj: &Map<String, Value>, key: &str) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    obj.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| format!("Missing \"{key}\""))
}

fn index(obj: &Map<String, Value>, key: &str) -> Result<usize, String> {
    obj.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| format!("Missing \"{key}\" (a nonnegative index)"))
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

fn known_fields(obj: &Map<String, Value>, fields: &[&str]) -> Result<(), String> {
    for key in obj.keys() {
        if !["cmd", "id", "token"].contains(&key.as_str()) && !fields.contains(&key.as_str()) {
            return Err(format!("Unknown field \"{key}\" for this command"));
        }
    }
    Ok(())
}

fn boolean(obj: &Map<String, Value>, key: &str, default: bool) -> Result<bool, String> {
    match obj.get(key) {
        None => Ok(default),
        Some(Value::Bool(v)) => Ok(*v),
        Some(_) => Err(format!("\"{key}\" must be true or false")),
    }
}

fn number(obj: &Map<String, Value>, key: &str, default: f32, lo: f32, hi: f32) -> Result<f32, String> {
    let Some(value) = obj.get(key) else { return Ok(default) };
    let n = value.as_f64().ok_or_else(|| format!("\"{key}\" must be a number"))? as f32;
    if !n.is_finite() || !(lo..=hi).contains(&n) {
        return Err(format!("\"{key}\" must be between {lo} and {hi}"));
    }
    Ok(n)
}

fn vector<const N: usize>(value: &Value, label: &str, limit: f32) -> Result<[f32; N], String> {
    let a = value.as_array().filter(|a| a.len() == N).ok_or_else(|| format!("{label} must contain {N} numbers"))?;
    let mut out = [0.0; N];
    for (v, target) in a.iter().zip(&mut out) {
        let n = v.as_f64().ok_or_else(|| format!("{label} must contain numbers"))? as f32;
        if !n.is_finite() || n.abs() > limit {
            return Err(format!("{label} must be finite and within ±{limit}"));
        }
        *target = n;
    }
    Ok(out)
}

fn points(value: &Value, cap: usize) -> Result<Vec<[f32; 2]>, String> {
    let a = value.as_array().ok_or("\"points\" must be [[x,y], ...]")?;
    if a.len() > cap {
        return Err(format!("A sketch can have up to {cap} points"));
    }
    a.iter().map(|p| vector(p, "Each point", sketch::COORD_MAX)).collect()
}

fn holes(value: &Value) -> Result<Vec<Vec<[f32; 2]>>, String> {
    let a = value.as_array().ok_or("\"holes\" must be an array of outlines")?;
    if a.len() > sketch::MAX_HOLES {
        return Err(format!("A sketch can have up to {} holes", sketch::MAX_HOLES));
    }
    a.iter().map(|p| points(p, MAX_OUTLINE)).collect()
}

fn plane(value: &Value) -> Result<Frame, String> {
    match value {
        Value::String(s) => match s.as_str() {
            "floor" => Ok(Frame::floor()),
            "front" => Ok(Frame::front()),
            "side" => Ok(Frame::side()),
            _ => Err("Unknown plane (floor, front, side, or {origin,u,v})".into()),
        },
        Value::Object(o) => {
            known_fields(o, &["origin", "u", "v"])?;
            let origin = vector(o.get("origin").ok_or("Missing plane origin")?, "Plane origin", 5000.0)?;
            let u = vector(o.get("u").ok_or("Missing plane u axis")?, "Plane u axis", 1.0e6)?;
            let v = vector(o.get("v").ok_or("Missing plane v axis")?, "Plane v axis", 1.0e6)?;
            Frame::from_axes(origin, u, v).ok_or_else(|| "Plane axes must be nonzero and not parallel".into())
        }
        _ => Err("\"plane\" must be floor, front, side, or {origin,u,v}".into()),
    }
}

fn optional_name(obj: &Map<String, Value>) -> Result<Option<&str>, String> {
    let Some(v) = obj.get("name") else { return Ok(None) };
    let s = v.as_str().filter(|s| !s.is_empty() && s.len() <= 256).ok_or("A name must contain 1–256 UTF-8 bytes")?;
    Ok(Some(s))
}

fn reference(obj: &Map<String, Value>, key: &str) -> Result<(usize, usize), String> {
    let r = obj.get(key).and_then(Value::as_object).ok_or_else(|| format!("\"{key}\" must be {{group,shape}}"))?;
    Ok((index(r, "group")?, index(r, "shape")?))
}

fn saved(scene: &Scene, at: (usize, usize)) -> Result<&Shape, String> {
    scene.shape(at.0, at.1).filter(|sh| sh.guide && sh.kind == Kind::Curve).ok_or_else(|| "No saved sketch at this group/shape index".into())
}

fn editable_group(scene: &Scene, g: usize) -> Result<(), String> {
    let group = scene.groups.get(g).ok_or("No such group")?;
    if group.instance_of.is_some() || group.sketch_holes {
        return Err("Choose an ordinary editable group, not an instance or a hole group".into());
    }
    Ok(())
}

/// All validation, including scene capacity, happens before adding a group or shape.
fn append_shapes(scene: &mut Scene, target: Option<usize>, mut shapes: Vec<Shape>, hole_group: Option<Op>) -> Result<Value, String> {
    let count = shapes.len();
    if count == 0 || scene.shape_count().saturating_add(count) > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    let g = if let Some(g) = target {
        editable_group(scene, g)?;
        g
    } else {
        if scene.groups.len() >= MAX_GROUPS {
            return Err(format!("Up to {MAX_GROUPS} groups"));
        }
        scene.groups.len()
    };
    if target.is_none() {
        let mut group = Group::new(if hole_group.is_some() { "Extrude with holes".to_owned() } else { format!("Group {}", g + 1) });
        if let Some(op) = hole_group {
            group.sketch_holes = true;
            group.op = op;
        }
        group.shapes.append(&mut shapes);
        scene.groups.push(group);
        Ok(json!({"group": g, "shape": 0, "shapes": count}))
    } else {
        let group = scene.groups.get_mut(g).ok_or("No such group")?;
        let first = group.shapes.len();
        group.shapes.append(&mut shapes);
        Ok(json!({"group": g, "shape": first, "shapes": count}))
    }
}

fn sketch_save(scene: &mut Scene, obj: &Map<String, Value>, replace: bool) -> Result<Value, String> {
    let mut fields = vec!["group", "points", "holes", "closed", "plane", "name"];
    if replace {
        fields.push("shape");
    }
    known_fields(obj, &fields)?;
    let target = if replace { Some((index(obj, "group")?, index(obj, "shape")?)) } else { None };
    if let Some((g, _)) = target {
        editable_group(scene, g)?;
    }
    let original = target.map(|at| saved(scene, at)).transpose()?;
    let closed = boolean(obj, "closed", original.is_none_or(|sh| crate::curve::settings(&sh.params).0))?;
    let points = points(obj.get("points").ok_or("Missing \"points\": [[x,y], ...]")?, if closed { MAX_OUTLINE } else { MAX_PATH })?;
    let holes = obj.get("holes").map(holes).transpose()?;
    let frame = obj.get("plane").map(plane).transpose()?;
    let name = optional_name(obj)?;
    let mut shape = if let Some(original) = original {
        sketch_saved::replace(original, frame, &points, holes.as_deref(), closed)?
    } else {
        sketch_saved::create(frame.unwrap_or_else(Frame::floor), &points, holes.as_deref().unwrap_or_default(), closed)?
    };
    if let Some(name) = name {
        shape.name = name.into();
    }
    if let Some((g, s)) = target {
        let dest = scene.shape_mut(g, s).ok_or("No such sketch")?;
        *dest = shape;
        return Ok(json!({"group": g, "shape": s}));
    }
    let group = if obj.contains_key("group") {
        let g = index(obj, "group")?;
        editable_group(scene, g)?;
        Some(g)
    } else {
        scene.groups.len().checked_sub(1).filter(|g| editable_group(scene, *g).is_ok())
    };
    append_shapes(scene, group, vec![shape], None)
}

fn solid_from_sketch(scene: &mut Scene, obj: &Map<String, Value>, command: &str) -> Result<Value, String> {
    let extra: &[&str] = match command {
        "solid.extrude" => &["height", "both", "flip"],
        "solid.revolve" => &[],
        "solid.loft" => &["top", "height", "top_scale", "flip"],
        "solid.sweep" => &["path", "radius", "scale"],
        _ => return Err("Unknown solid operation".into()),
    };
    let fields: Vec<_> = ["sketch", "op", "name", "color"].iter().chain(extra.iter()).copied().collect();
    known_fields(obj, &fields)?;
    let source = reference(obj, "sketch")?;
    let sh = saved(scene, source)?;
    let op = match obj.get("op") {
        None => Op::Add,
        Some(Value::String(s)) if s == "Add" => Op::Add,
        Some(Value::String(s)) if s == "Subtract" => Op::Subtract,
        _ => return Err("\"op\" must be Add or Subtract".into()),
    };
    let color = if let Some(v) = obj.get("color") {
        let c: [f32; 3] = vector(v, "Color", 1.0)?;
        if c.iter().any(|v| *v < 0.0) {
            return Err("Color channels must be between 0 and 1".into());
        }
        c
    } else if op == Op::Subtract {
        [0.9, 0.35, 0.4]
    } else {
        crate::sketch_ui::COLORS[scene.shape_count() % crate::sketch_ui::COLORS.len()]
    };
    let name = optional_name(obj)?;
    let mut cutouts = Vec::new();
    let built = if command == "solid.sweep" && !crate::curve::settings(&sh.params).0 {
        if sh.guide_plane.as_ref().is_some_and(|plane| !plane.holes.is_empty()) {
            return Err("An open sketch path cannot have holes".into());
        }
        if obj.contains_key("path") || obj.contains_key("scale") {
            return Err("An open sketch is the sweep path; use radius for a round sweep".into());
        }
        let path = sketch_saved::open_path(sh)?;
        let radius = number(obj, "radius", 4.0, 0.4, 16.0)?;
        let profile = sketch::circle([0.0, 0.0], 4.0, sketch::CIRCLE_POINTS);
        sketch::sweep(&profile, &path, radius / 4.0).map_err(|e| e.message())?
    } else {
        let (frame, profile) = sketch_saved::profile(sh).ok_or("Choose a closed sketch on one plane")?;
        let holes = sketch::validate_holes(&profile, &sketch_saved::holes(sh, &frame)).map_err(|e| e.message())?;
        if !holes.is_empty() && command != "solid.extrude" {
            return Err("Sketches with holes can only be extruded".into());
        }
        match command {
            "solid.extrude" => {
                let height = number(obj, "height", 20.0, 0.1, 2000.0)?;
                let flip = boolean(obj, "flip", false)?;
                let both = boolean(obj, "both", false)?;
                let build = |p: &[[f32; 2]]| -> Result<sketch::Built, String> {
                    let mut b = sketch::extrude(&frame, p, height, flip).map_err(|e| e.message())?;
                    if both {
                        b.pos = frame.to_world(sketch::centre(p));
                    }
                    Ok(b)
                };
                for hole in &holes {
                    cutouts.push(build(hole)?);
                }
                build(&profile)?
            }
            "solid.revolve" => sketch::revolve(&frame, &profile).map_err(|e| e.message())?,
            "solid.loft" => {
                let (top, height, flip) = if obj.contains_key("top") {
                    if ["height", "top_scale", "flip"].iter().any(|key| obj.contains_key(*key)) {
                        return Err("A saved top sketch sets the loft's position, distance and direction".into());
                    }
                    let top = reference(obj, "top")?;
                    if top == source {
                        return Err("Choose a different top sketch".into());
                    }
                    sketch_saved::loft_top(&frame, &profile, saved(scene, top)?)?
                } else {
                    (
                        sketch::scaled(&profile, number(obj, "top_scale", 0.5, 0.1, 3.0)?),
                        number(obj, "height", 20.0, 0.1, 2000.0)?,
                        boolean(obj, "flip", false)?,
                    )
                };
                sketch::loft(&frame, &profile, &top, height, flip).map_err(|e| e.message())?
            }
            "solid.sweep" => {
                if obj.contains_key("radius") {
                    return Err("A profile sweep uses scale; radius is for a round sweep".into());
                }
                let path = reference(obj, "path")?;
                if path == source {
                    return Err("Choose a different open sketch for the path".into());
                }
                let path = sketch_saved::open_path(saved(scene, path)?)?;
                sketch::sweep(&profile, &path, number(obj, "scale", 1.0, 0.2, 4.0)?).map_err(|e| e.message())?
            }
            _ => return Err("Unknown solid operation".into()),
        }
    };
    if std::iter::once(&built).chain(cutouts.iter()).any(|b| !b.pos.iter().all(|v| v.is_finite() && v.abs() <= 5000.0)) {
        return Err("The solid's position is outside the scene limits".into());
    }
    let mut solid = Shape::new(built.kind, built.pos, Material { color, ..Material::default() });
    sketch::apply(&built, &mut solid);
    solid.name = name.unwrap_or(built.kind.label()).into();
    solid.op = if cutouts.is_empty() { op } else { Op::Add };
    solid.blend = 0.0;
    let material = solid.material;
    let mut shapes = vec![solid];
    for (i, hole) in cutouts.iter().enumerate() {
        let mut cutter = Shape::new(hole.kind, hole.pos, material);
        sketch::apply(hole, &mut cutter);
        cutter.name = format!("Hole {}", i + 1);
        cutter.op = Op::Subtract;
        cutter.blend = 0.0;
        shapes.push(cutter);
    }
    let hole_group = (!cutouts.is_empty()).then_some(op);
    let target = hole_group.is_none().then_some(source.0).filter(|g| editable_group(scene, *g).is_ok());
    append_shapes(scene, target, shapes, hole_group)
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

    fn empty_scene() -> Scene {
        Scene { groups: Vec::new(), ..Scene::default() }
    }

    fn shape_at<'a>(scene: &'a Scene, at: &Value) -> &'a Shape {
        scene.shape(at["group"].as_u64().expect("group") as usize, at["shape"].as_u64().expect("shape") as usize).expect("created shape")
    }

    fn assert_built(shape: &Shape, built: &sketch::Built) {
        assert_eq!(shape.kind, built.kind);
        assert_eq!(shape.pos, built.pos);
        assert_eq!(shape.rot, built.rot);
        assert_eq!(shape.size, built.size);
        assert_eq!(shape.outline, built.outline);
        assert_eq!(shape.outline2, built.outline2);
        assert_eq!(shape.path, built.path);
    }

    #[test]
    fn finished_sketch_keeps_all_vertices_plane_and_holes_after_reload() {
        let mut sc = empty_scene();
        let outline = sketch::circle([25.0, 10.0], 12.0, MAX_OUTLINE);
        let hole = sketch::circle([25.0, 10.0], 3.0, 16);
        let at = run(
            &mut sc,
            json!({"cmd":"sketch.create", "name":"Tilted ring", "points":outline, "holes":[hole],
                "plane":{"origin":[40,30,20], "u":[1,1,0], "v":[-1,1,2]}}),
        )
        .expect("finished sketch");
        assert!(shape_at(&sc, &at).guide);
        assert_eq!(shape_at(&sc, &at).path.len(), MAX_OUTLINE);
        let back = Scene::from_json(&sc.try_to_json().expect("serialize")).expect("reload");
        let original = shape_at(&sc, &at);
        let sh = shape_at(&back, &at);
        assert_eq!(sh.path, original.path);
        let before_plane = original.guide_plane.as_ref().expect("saved plane");
        let after_plane = sh.guide_plane.as_ref().expect("reloaded plane");
        assert_eq!(after_plane.holes, before_plane.holes, "all hole vertices survive exactly");
        assert_eq!(after_plane.holes, vec![hole]);
        // Loading reorthonormalizes the axes. Repeated f32 normalization can change
        // their last bits, but must retain their directions and the world contours.
        assert!(
            after_plane
                .u
                .iter()
                .chain(&after_plane.v)
                .zip(before_plane.u.iter().chain(&before_plane.v))
                .all(|(a, b)| (a - b).abs() <= 4.0 * f32::EPSILON)
        );
        let (frame, profile) = sketch_saved::profile(sh).expect("profile");
        let (before_frame, before_profile) = sketch_saved::profile(original).expect("original profile");
        assert_eq!(frame.origin, [40.0, 30.0, 20.0]);
        assert_eq!(profile.len(), MAX_OUTLINE);
        let after_holes = sketch_saved::holes(sh, &frame);
        let before_holes = sketch_saved::holes(original, &before_frame);
        assert_eq!(after_holes.len(), 1);
        assert_eq!(after_holes.len(), before_holes.len());
        for (after, before) in std::iter::once(&profile).chain(&after_holes).zip(std::iter::once(&before_profile).chain(&before_holes)) {
            assert_eq!(after.len(), before.len());
            for (a, b) in after.iter().zip(before) {
                assert!(
                    crate::sdf::len(crate::sdf::sub(frame.to_world(*a), before_frame.to_world(*b))) < 1e-4,
                    "reloading must preserve each contour's world vertices within 0.0001 mm"
                );
            }
        }
        assert!(profile.iter().zip(&outline).all(|(a, b)| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4));
        assert_eq!(summary(&back)["groups"][0]["shapes"][0]["sketch"], true);
    }

    #[test]
    fn editing_a_transformed_saved_sketch_preserves_omitted_metadata() {
        let mut sc = empty_scene();
        let at = run(
            &mut sc,
            json!({"cmd":"sketch.create", "name":"Keep me", "points":[[-10,-10],[10,-10],[10,10],[-10,10]],
                "holes":[[[-2,-2],[2,-2],[2,2],[-2,2]]]}),
        )
        .expect("saved sketch");
        run(
            &mut sc,
            json!({"cmd":"shape.set", "group":0, "shape":0, "pos":[30,40,50], "rot":[15,20,35],
                "scale":[1.5,0.8,1.2], "color":[0.1,0.2,0.9], "visible":false, "source":"kept-source"}),
        )
        .expect("transform");
        let before = shape_at(&sc, &at).clone();
        let (frame, old) = sketch_saved::profile(&before).expect("transformed profile");
        let edited = sketch::scaled(&old, 1.2);
        run(&mut sc, json!({"cmd":"sketch.set", "group":0, "shape":0, "points":edited})).expect("edit");
        let after = shape_at(&sc, &at);
        assert_eq!(
            (after.uid, &after.name, after.material, after.visible, &after.source),
            (before.uid, &before.name, before.material, before.visible, &before.source)
        );
        assert_eq!((after.pos, after.rot, after.scale), (before.pos, before.rot, before.scale));
        assert_eq!(after.guide_plane, before.guide_plane, "omitted plane and holes remain exactly stored");
        let (next_frame, points) = sketch_saved::profile(after).expect("edited profile");
        assert_eq!(next_frame.origin, frame.origin);
        assert!(points.iter().zip(&edited).all(|(a, b)| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4));
        let stable = sc.clone();
        assert!(run(&mut sc, json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[0,0],[1,0],[0,1]]})).is_err());
        assert_eq!(sc, stable, "an edit that would lose an existing hole is atomic");
        run(
            &mut sc,
            json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[-20,-20],[20,-20],[20,20],[-20,20]],
                "plane":{"origin":[50,20,-10], "u":[0,1,0], "v":[0,0,1]}}),
        )
        .expect("move the sketch plane");
        let moved = shape_at(&sc, &at);
        assert_eq!(
            (moved.uid, moved.rot, moved.scale, moved.material, moved.visible),
            (before.uid, before.rot, before.scale, before.material, before.visible)
        );
        let (frame, outline) = sketch_saved::profile(moved).expect("relocated plane");
        assert_eq!(frame.origin, [50.0, 20.0, -10.0]);
        assert!((frame.u[1] - 1.0).abs() < 1e-4 && (frame.v[2] - 1.0).abs() < 1e-4);
        assert!((sketch::signed_area(&outline) - 1600.0).abs() < 0.02);
        assert_eq!(sketch_saved::holes(moved, &frame).len(), 1);
    }

    #[test]
    fn saved_sketch_extrusion_keeps_holes_and_both_sides_and_supports_cut() {
        let mut sc = empty_scene();
        let at = run(
            &mut sc,
            json!({"cmd":"sketch.create", "points":[[-10,-8],[10,-8],[10,8],[-10,8]],
                "holes":[[[-2,-2],[2,-2],[2,2],[-2,2]]]}),
        )
        .expect("saved sketch");
        let source = shape_at(&sc, &at).clone();
        let made = run(&mut sc, json!({"cmd":"solid.extrude", "sketch":at, "height":12, "both":true})).expect("extrude");
        let group = sc.groups.get(made["group"].as_u64().expect("group") as usize).expect("hole group");
        assert!(group.sketch_holes);
        assert_eq!(group.shapes.len(), 2);
        assert_eq!(group.shapes[0].op, Op::Add);
        assert_eq!(group.shapes[1].op, Op::Subtract);
        assert!(group.shapes.iter().all(|s| s.pos[2] == 0.0 && s.size[0] == 12.0 && s.blend == 0.0));
        let field = crate::sdf::Field::new(&sc);
        assert!(field.distance([5.0, 0.0, 0.0]) < 0.0);
        assert!(field.distance([0.0, 0.0, 0.0]) > 0.0, "the central hole stays empty");
        let cut = run(&mut sc, json!({"cmd":"solid.extrude", "sketch":at, "height":4, "op":"Subtract"})).expect("cut");
        assert_eq!(sc.groups[cut["group"].as_u64().expect("group") as usize].op, Op::Subtract);
        assert_eq!(shape_at(&sc, &at), &source, "solid commands never consume the guide");
    }

    #[test]
    fn saved_sketch_revolve_and_loft_use_the_same_geometry_and_actual_planes() {
        let mut sc = empty_scene();
        let standing = run(
            &mut sc,
            json!({"cmd":"sketch.create", "points":[[3,-4],[9,-4],[9,8],[3,8]],
                "plane":{"origin":[30,40,50], "u":[1,0,0], "v":[0,0,1]}}),
        )
        .expect("standing sketch");
        let (frame, profile) = sketch_saved::profile(shape_at(&sc, &standing)).expect("profile");
        let expected = sketch::revolve(&frame, &profile).expect("revolve geometry");
        let made = run(&mut sc, json!({"cmd":"solid.revolve", "sketch":standing})).expect("revolve command");
        assert_built(shape_at(&sc, &made), &expected);
        assert_eq!(shape_at(&sc, &made).pos, [30.0, 40.0, 52.0]);
        let base = run(&mut sc, json!({"cmd":"sketch.create", "points":[[-10,-10],[10,-10],[10,10],[-10,10]]})).expect("base");
        let top = run(
            &mut sc,
            json!({"cmd":"sketch.create", "points":[[-5,-5],[5,-5],[5,5],[-5,5]],
                "plane":{"origin":[6,-3,30], "u":[-1,0,0], "v":[0,-1,0]}}),
        )
        .expect("top");
        let before_base = shape_at(&sc, &base).clone();
        let before_top = shape_at(&sc, &top).clone();
        let made = run(&mut sc, json!({"cmd":"solid.loft", "sketch":base, "top":top})).expect("saved loft");
        let loft = shape_at(&sc, &made);
        assert_eq!(loft.kind, Kind::Loft);
        assert_eq!(loft.size[0], 30.0);
        assert_eq!(loft.pos, [0.0, 0.0, 15.0]);
        assert_eq!(sketch::centre(&loft.outline2), [6.0, -3.0]);
        assert_eq!(shape_at(&sc, &base), &before_base);
        assert_eq!(shape_at(&sc, &top), &before_top);
        let scaled = run(&mut sc, json!({"cmd":"solid.loft", "sketch":base, "height":10, "top_scale":0.5, "flip":true})).expect("scaled loft");
        let loft = shape_at(&sc, &scaled);
        assert_eq!(loft.pos[2], -5.0);
        assert_eq!(sketch::signed_area(&loft.outline).abs(), 100.0, "the smaller far profile is the local bottom when flipped");
        assert_eq!(sketch::signed_area(&loft.outline2).abs(), 400.0);
    }

    #[test]
    fn saved_sketch_sweep_uses_an_open_path_or_a_round_profile() {
        let mut sc = empty_scene();
        let profile = run(&mut sc, json!({"cmd":"sketch.create", "points":[[-3,-2],[3,-2],[3,2],[-3,2]]})).expect("profile");
        let path = run(
            &mut sc,
            json!({"cmd":"sketch.create", "closed":false, "points":[[0,0],[0,20],[10,25]],
                "plane":{"origin":[2,3,4], "u":[1,0,0], "v":[0,0,1]}}),
        )
        .expect("path");
        let source = shape_at(&sc, &path).clone();
        let made = run(&mut sc, json!({"cmd":"solid.sweep", "sketch":profile, "path":path, "scale":2})).expect("profile sweep");
        let sweep = shape_at(&sc, &made);
        let world: Vec<_> = sweep.path.iter().map(|p| crate::curve_tool::to_world(sweep, [p[0], p[1], p[2]])).collect();
        assert_eq!(world, vec![[2.0, 3.0, 4.0], [2.0, 3.0, 24.0], [12.0, 3.0, 29.0]]);
        assert!(sweep.path.iter().all(|p| p[3] == 2.0));
        let round = run(&mut sc, json!({"cmd":"solid.sweep", "sketch":path, "radius":8})).expect("round sweep");
        let sweep = shape_at(&sc, &round);
        assert_eq!(sweep.outline.len(), sketch::CIRCLE_POINTS);
        assert!(sweep.path.iter().all(|p| p[3] == 2.0), "round UI uses a radius-4 profile scaled by radius / 4");
        assert_eq!(shape_at(&sc, &path), &source);
    }

    #[test]
    fn saved_sketch_commands_validate_capacity_before_mutating_the_scene() {
        let mut sc = empty_scene();
        let at =
            run(&mut sc, json!({"cmd":"sketch.create", "points":[[-10,-10],[10,-10],[10,10],[-10,10]], "holes":[[[-2,-2],[2,-2],[2,2],[-2,2]]]}))
                .expect("sketch");
        sc.groups[0].shapes.resize_with(MAX_SHAPES - 1, || Shape::new(Kind::Box, [0.0; 3], Material::default()));
        let before = sc.clone();
        assert!(run(&mut sc, json!({"cmd":"solid.extrude", "sketch":at})).is_err());
        assert_eq!(sc, before, "the outer solid cannot be added without room for its hole");
        sc.groups[0].shapes.truncate(1);
        sc.groups.resize_with(MAX_GROUPS, || Group::new("Full"));
        let before = sc.clone();
        assert!(run(&mut sc, json!({"cmd":"solid.extrude", "sketch":at})).is_err());
        assert_eq!(sc, before, "no partial hole group at the group limit");
        let mut empty = empty_scene();
        let before = empty.clone();
        assert!(run(&mut empty, json!({"cmd":"sketch.create", "group":99, "points":[[0,0],[10,0],[0,10]]})).is_err());
        assert_eq!(empty, before, "bad target must not even add an empty group");

        run(&mut empty, json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]]})).expect("saved sketch");
        for linked in [false, true] {
            let mut protected = empty.clone();
            protected.groups[0].sketch_holes = !linked;
            protected.groups[0].instance_of = linked.then_some(u32::MAX);
            let before = protected.clone();
            assert!(run(&mut protected, json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[0,0],[20,0],[0,20]]})).is_err());
            assert_eq!(protected, before, "a linked copy or a hole group must reject editing without sanitizing away its saved sketch");
        }
    }

    #[test]
    fn hostile_commands_return_errors_and_never_panic() {
        let mut sc = Scene::default();
        for bad in [
            json!(5),
            json!({}),
            json!({"cmd": 3}),
            json!({"cmd": "nope"}),
            json!({"cmd": "piece.reload"}),
            json!({"cmd": "piece.reload", "group": 99999}),
            json!({"cmd": "piece.reload", "group": 0}),
            json!({"cmd": "piece.save", "group": 0}),
            json!({"cmd": "piece.save", "group": 0, "path": "/nonexistent/dir/x.meltcad"}),
            json!({"cmd": "scene.add_file"}),
            json!({"cmd": "scene.add_file", "path": 7}),
            json!({"cmd": "scene.add_file", "path": "/nonexistent/meltcad-add-file.meltcad"}),
            json!({"cmd": "shape.add"}),
            json!({"cmd": "shape.add", "kind": "Banana"}),
            json!({"cmd": "shape.add", "kind": "Box", "size": "big"}),
            json!({"cmd": "shape.set", "group": 99, "shape": 0}),
            json!({"cmd": "shape.delete", "group": -1, "shape": 0}),
            json!({"cmd": "extrude", "points": [[0, 0], [1]]}),
            json!({"cmd": "extrude", "points": [[0, 0], [1, 1], [2, 0]], "plane": "moon"}),
            json!({"cmd": "melt.set", "amount": "lots"}),
            json!({"cmd": "save"}),
            json!({"cmd": "sketch.create"}),
            json!({"cmd": "sketch.set", "group": 0, "shape": 0, "points": [[0,0],[10,0],[0,10]]}),
            json!({"cmd": "solid.extrude", "sketch": {"group": -1, "shape": 0}}),
            json!({"cmd": "solid.revolve", "sketch": {"group": 0, "shape": u64::MAX}}),
            json!({"cmd": "solid.loft", "sketch": null}),
            json!({"cmd": "solid.sweep"}),
            json!({"cmd": "anim.at"}),
            json!({"cmd": "anim.at", "frame": -1}),
            json!({"cmd": "anim.at", "frame": 1.5}),
            json!({"cmd": "anim.at", "frame": u64::MAX}),
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

        // Valid saved inputs exercise malformed parameters past reference lookup.
        let mut sc = empty_scene();
        let base = run(&mut sc, json!({"cmd":"sketch.create", "points":[[-10,-10],[10,-10],[10,10],[-10,10]]})).expect("base");
        let upright = run(&mut sc, json!({"cmd":"sketch.create", "plane":"front", "points":[[2,0],[10,0],[10,20],[2,20]]})).expect("upright");
        let path = run(&mut sc, json!({"cmd":"sketch.create", "closed":false, "points":[[0,0],[0,20],[10,30]]})).expect("path");
        let holed =
            run(&mut sc, json!({"cmd":"sketch.create", "points":[[-10,-10],[10,-10],[10,10],[-10,10]], "holes":[[[-2,-2],[2,-2],[2,2],[-2,2]]]}))
                .expect("holed");
        for bad in [
            json!({"cmd":"sketch.create", "points":vec![[0,0]; MAX_OUTLINE + 1]}),
            json!({"cmd":"sketch.create", "closed":false, "points":vec![[0,0]; MAX_PATH + 1]}),
            json!({"cmd":"sketch.create", "points":[[0,0],[1e300,0],[0,10]]}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,10],[0,10],[10,0]]}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]], "closed":"yes"}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]], "plane":"moon"}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]], "plane":{"origin":[0,0,0], "u":[0,0,0], "v":[0,1,0]}}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]], "plane":{"origin":[1e300,0,0], "u":[1,0,0], "v":[0,1,0]}}),
            json!({"cmd":"sketch.create", "points":[[0,0],[10,0],[0,10]], "holes":vec![vec![[1,1],[2,1],[1,2]]; sketch::MAX_HOLES + 1]}),
            json!({"cmd":"sketch.create", "closed":false, "points":[[0,0],[10,0]], "holes":[[[1,1],[2,1],[1,2]]]}),
            json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[0,null],[10,0],[0,10]]}),
            json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[0,0],[10,0],[0,10]], "plane":false}),
            json!({"cmd":"sketch.set", "group":0, "shape":0, "points":[[0,0],[10,0],[0,10]], "name":"x".repeat(257)}),
            json!({"cmd":"sketch.set", "group":0, "shape":3, "points":[[0,0],[10,0]], "closed":false}),
            json!({"cmd":"solid.extrude", "sketch":base, "height":0}),
            json!({"cmd":"solid.extrude", "sketch":base, "height":1e300}),
            json!({"cmd":"solid.extrude", "sketch":base, "height":"ten"}),
            json!({"cmd":"solid.extrude", "sketch":base, "both":"yes"}),
            json!({"cmd":"solid.extrude", "sketch":base, "op":"Paint"}),
            json!({"cmd":"solid.extrude", "sketch":base, "color":[1,-0.1,0]}),
            json!({"cmd":"solid.revolve", "sketch":base}),
            json!({"cmd":"solid.revolve", "sketch":holed}),
            json!({"cmd":"solid.loft", "sketch":base, "top":upright}),
            json!({"cmd":"solid.loft", "sketch":base, "top":base}),
            json!({"cmd":"solid.loft", "sketch":base, "top":upright, "height":20}),
            json!({"cmd":"solid.loft", "sketch":base, "top_scale":0}),
            json!({"cmd":"solid.sweep", "sketch":base}),
            json!({"cmd":"solid.sweep", "sketch":base, "path":upright}),
            json!({"cmd":"solid.sweep", "sketch":base, "path":path, "scale":0}),
            json!({"cmd":"solid.sweep", "sketch":path, "radius":0}),
            json!({"cmd":"solid.sweep", "sketch":path, "scale":2}),
            json!({"cmd":"solid.sweep", "sketch":holed, "path":path}),
        ] {
            let before = sc.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut sc, bad.clone())));
            assert!(matches!(result, Ok(Err(_))), "hostile command must return an error without panicking: {bad}");
            assert_eq!(sc, before, "failed command must keep the scene unchanged: {bad}");
        }
    }

    #[test]
    fn animation_at_reports_and_applies_the_same_clamped_frame() {
        let mut sc = Scene::default();
        run(&mut sc, json!({"cmd": "anim.set", "length": 10})).expect("animation length");
        sc.groups[0].shapes[0].pos = [0.0; 3];
        run(&mut sc, json!({"cmd": "anim.key", "frame": 0})).expect("first pose");
        sc.groups[0].shapes[0].pos = [90.0, 0.0, 0.0];
        run(&mut sc, json!({"cmd": "anim.key", "frame": 9})).expect("last pose");
        let keys = sc.anim.clone();
        let reply = run(&mut sc, json!({"cmd": "anim.at", "frame": 4})).expect("middle pose");
        assert_eq!(reply["frame"], 4);
        assert!((sc.groups[0].shapes[0].pos[0] - 40.0).abs() < 1e-5);
        let reply = run(&mut sc, json!({"cmd": "anim.at", "frame": u32::MAX})).expect("clamped pose");
        assert_eq!(reply["frame"], 9);
        assert_eq!(sc.groups[0].shapes[0].pos, [90.0, 0.0, 0.0]);
        assert_eq!(sc.anim, keys, "showing a pose never records another key");
    }

    #[test]
    fn animation_commands_pose_blend_and_render() {
        let mut sc = Scene::default();
        run(&mut sc, json!({"cmd": "anim.set", "length": 10, "fps": 10})).unwrap();
        run(&mut sc, json!({"cmd": "anim.key", "frame": 0})).unwrap();
        run(&mut sc, json!({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [40, 0, 20]})).unwrap();
        run(&mut sc, json!({"cmd": "melt.set", "amount": 3})).unwrap();
        run(&mut sc, json!({"cmd": "anim.key", "frame": 8})).unwrap();
        run(&mut sc, json!({"cmd": "anim.at", "frame": 4})).unwrap();
        assert!((sc.groups[0].shapes[0].pos[0] - 20.0).abs() < 1e-3);
        assert_eq!(run(&mut sc, json!({"cmd": "anim.get"})).unwrap()["poses"], json!([0, 8]));
        assert!(run(&mut sc, json!({"cmd": "anim.key"})).is_err());
        let p = std::env::temp_dir().join(format!("melt-anim-{}.gif", std::process::id()));
        run(&mut sc, json!({"cmd": "anim.render", "path": p.to_string_lossy(), "size": 64})).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        let _ = std::fs::remove_file(&p);
        run(&mut sc, json!({"cmd": "anim.clear"})).unwrap();
        assert!(run(&mut sc, json!({"cmd": "anim.render", "path": "x.gif"})).is_err());
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
