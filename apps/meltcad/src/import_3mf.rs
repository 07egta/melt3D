//! 3MF import. A 3MF file is a ZIP (read with `import_inflate::Zip`) whose `3D/3dmodel.model` is
//! XML: objects with meshes (vertices and triangles) or components (other objects with a
//! transform), and build items (the objects to print, with a transform). The `unit` attribute
//! becomes millimetres. Objects stored in other model files of the package (the production
//! extension's `p:path`) are followed. The XML reader is our own and small: it needs no entity
//! expansion, so a hostile file cannot blow up through entities.

use std::collections::HashMap;

use crate::import_inflate::Zip;
use crate::import_mesh::{
    Affine, BAD_INDEX, BAD_NUMBER, CUT_OFF, ImportedMesh, MAX_TRIANGLES, MAX_VERTICES, NO_SHAPE, TOO_MANY_TRIS, TOO_MANY_VERTS, append_transformed,
    parse_f32,
};

/// The biggest model XML we unpack (bytes). The tests use a small cap so that the "bomb" test is light.
#[cfg(not(test))]
const MAX_MODEL_BYTES: usize = 400 * 1024 * 1024;
#[cfg(test)]
const MAX_MODEL_BYTES: usize = 8 * 1024 * 1024;
/// All model files of a package together.
#[cfg(not(test))]
const MAX_TOTAL_XML: usize = 600 * 1024 * 1024;
#[cfg(test)]
const MAX_TOTAL_XML: usize = 12 * 1024 * 1024;
/// Component nesting.
const MAX_DEPTH: usize = 16;
/// Objects visited while building (components can multiply).
const MAX_VISITS: usize = 200_000;
const MAX_OBJECTS: usize = 100_000;
const MAX_DOCS: usize = 64;

const BAD_XML: &str = "The 3MF model file (XML) is broken";

// ---------------------------------------------------------------- a tiny XML tag reader

struct Tag<'a> {
    /// The name without a namespace prefix.
    name: &'a [u8],
    attrs: &'a [u8],
    closing: bool,
    self_closing: bool,
}

impl<'a> Tag<'a> {
    /// The value of an attribute (by its full name, or by its local name after a prefix).
    fn attr(&self, key: &str) -> Option<&'a str> {
        let a = self.attrs;
        let mut i = 0;
        while i < a.len() {
            while a.get(i).is_some_and(|c| c.is_ascii_whitespace() || *c == b'/') {
                i += 1;
            }
            let ns = i;
            while a.get(i).is_some_and(|c| !c.is_ascii_whitespace() && *c != b'=') {
                i += 1;
            }
            let name = a.get(ns..i)?;
            while a.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if a.get(i) != Some(&b'=') {
                continue;
            }
            i += 1;
            while a.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            let q = *a.get(i)?;
            if q != b'"' && q != b'\'' {
                return None;
            }
            i += 1;
            let vs = i;
            while a.get(i).is_some_and(|c| *c != q) {
                i += 1;
            }
            let value = a.get(vs..i)?;
            i += 1;
            let local = name.rsplit(|c| *c == b':').next().unwrap_or(name);
            let key_local = key.rsplit(':').next().unwrap_or(key);
            if name == key.as_bytes() || local == key_local.as_bytes() {
                return std::str::from_utf8(value).ok();
            }
        }
        None
    }
}

/// The next tag after `pos` (comments, processing instructions, CDATA and DOCTYPE are skipped).
fn next_tag<'a>(src: &'a [u8], pos: &mut usize) -> Result<Option<Tag<'a>>, &'static str> {
    loop {
        let Some(off) = src.get(*pos..).and_then(|s| s.iter().position(|c| *c == b'<')) else {
            *pos = src.len();
            return Ok(None);
        };
        let start = *pos + off;
        let rest = src.get(start..).ok_or(BAD_XML)?;
        let skip_to = |end: &[u8]| rest.windows(end.len()).position(|w| w == end).map(|p| start + p + end.len());
        if rest.starts_with(b"<!--") {
            *pos = skip_to(b"-->").ok_or(BAD_XML)?;
            continue;
        }
        if rest.starts_with(b"<?") {
            *pos = skip_to(b"?>").ok_or(BAD_XML)?;
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            *pos = skip_to(b"]]>").ok_or(BAD_XML)?;
            continue;
        }
        if rest.starts_with(b"<!") {
            // DOCTYPE: skip to the closing '>' (outside an internal subset in brackets)
            let mut depth = 0i32;
            let mut k = 2;
            loop {
                match rest.get(k) {
                    None => return Err(BAD_XML),
                    Some(b'[') => depth += 1,
                    Some(b']') => depth -= 1,
                    Some(b'>') if depth <= 0 => break,
                    _ => {}
                }
                k += 1;
            }
            *pos = start + k + 1;
            continue;
        }
        // an ordinary tag: find its '>' outside of quotes
        let mut k = 1;
        let mut quote = 0u8;
        let end = loop {
            match rest.get(k) {
                None => return Err(BAD_XML),
                Some(&c) if quote != 0 && c == quote => quote = 0,
                Some(_) if quote != 0 => {}
                Some(&c) if c == b'"' || c == b'\'' => quote = c,
                Some(b'>') => break k,
                _ => {}
            }
            k += 1;
        };
        *pos = start + end + 1;
        let inner = rest.get(1..end).ok_or(BAD_XML)?;
        let closing = inner.first() == Some(&b'/');
        let inner = if closing { inner.get(1..).ok_or(BAD_XML)? } else { inner };
        let self_closing = inner.last() == Some(&b'/');
        let name_end = inner.iter().position(|c| c.is_ascii_whitespace() || *c == b'/').unwrap_or(inner.len());
        let full = inner.get(..name_end).ok_or(BAD_XML)?;
        let name = full.rsplit(|c| *c == b':').next().unwrap_or(full);
        let attrs = inner.get(name_end..).ok_or(BAD_XML)?;
        return Ok(Some(Tag { name, attrs, closing, self_closing }));
    }
}

// ---------------------------------------------------------------- the model

struct Comp {
    id: u32,
    path: Option<String>,
    m: Affine,
}

#[derive(Default)]
struct Obj {
    /// Vertex range in `Doc::verts` and triangle range in `Doc::tris` (indices are local).
    v: std::ops::Range<usize>,
    t: std::ops::Range<usize>,
    comps: Vec<Comp>,
    skip: bool,
}

#[derive(Default)]
struct Doc {
    unit_mm: f64,
    verts: Vec<[f32; 3]>,
    tris: Vec<[u32; 3]>,
    objects: HashMap<u32, Obj>,
    /// (object, file, transform)
    build: Vec<(u32, Option<String>, Affine)>,
}

fn unit_mm(s: &str) -> f64 {
    match s.trim().to_ascii_lowercase().as_str() {
        "micron" => 0.001,
        "millimeter" => 1.0,
        "centimeter" => 10.0,
        "inch" => 25.4,
        "foot" => 304.8,
        "meter" => 1000.0,
        _ => 1.0,
    }
}

/// `m00 m01 m02 m10 m11 m12 m20 m21 m22 m30 m31 m32`: row vectors, so `p' = p M + t`.
fn parse_transform(s: &str) -> Result<Affine, String> {
    let v: Vec<f64> = s.split_whitespace().map(|t| t.parse::<f64>().map_err(|_| BAD_NUMBER.to_string())).collect::<Result<_, _>>()?;
    if v.len() != 12 || !v.iter().all(|x| x.is_finite() && x.abs() < 1.0e12) {
        return Err(BAD_NUMBER.into());
    }
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    Ok(Affine([[g(0), g(3), g(6), g(9)], [g(1), g(4), g(7), g(10)], [g(2), g(5), g(8), g(11)]]))
}

fn id_attr(t: &Tag, key: &str) -> Result<u32, String> {
    t.attr(key).and_then(|s| s.trim().parse::<u32>().ok()).ok_or_else(|| BAD_XML.to_string())
}

fn coord(t: &Tag, key: &str) -> Result<f32, String> {
    t.attr(key).and_then(|s| parse_f32(s.trim().as_bytes())).filter(|v| v.is_finite()).ok_or_else(|| BAD_NUMBER.to_string())
}

fn parse_doc(src: &[u8], total_verts: &mut usize, total_tris: &mut usize) -> Result<Doc, String> {
    let mut doc = Doc { unit_mm: 1.0, ..Doc::default() };
    let mut pos = 0usize;
    let mut cur: Option<(u32, Obj)> = None;
    let mut in_build = false;
    while let Some(t) = next_tag(src, &mut pos).map_err(str::to_string)? {
        match (t.name, t.closing) {
            (b"model", false) => {
                if let Some(u) = t.attr("unit") {
                    doc.unit_mm = unit_mm(u);
                }
            }
            (b"object", false) => {
                let id = id_attr(&t, "id")?;
                let ty = t.attr("type").unwrap_or("model");
                let obj = Obj {
                    v: doc.verts.len()..doc.verts.len(),
                    t: doc.tris.len()..doc.tris.len(),
                    skip: ty == "support" || ty == "solidsupport",
                    ..Obj::default()
                };
                if t.self_closing {
                    doc.objects.insert(id, obj);
                } else {
                    cur = Some((id, obj));
                }
                if doc.objects.len() > MAX_OBJECTS {
                    return Err("Too many shapes in the 3MF".into());
                }
            }
            (b"object", true) => {
                if let Some((id, mut obj)) = cur.take() {
                    obj.v.end = doc.verts.len();
                    obj.t.end = doc.tris.len();
                    let n = (obj.v.end - obj.v.start) as u32;
                    if doc.tris.get(obj.t.clone()).is_some_and(|ts| ts.iter().flatten().any(|i| *i >= n)) {
                        return Err(BAD_INDEX.into());
                    }
                    doc.objects.insert(id, obj);
                }
            }
            (b"vertex", false) if cur.is_some() => {
                if *total_verts >= MAX_VERTICES {
                    return Err(TOO_MANY_VERTS.into());
                }
                *total_verts += 1;
                let p = [coord(&t, "x")?, coord(&t, "y")?, coord(&t, "z")?];
                doc.verts.push(p);
            }
            (b"triangle", false) if cur.is_some() => {
                if *total_tris >= MAX_TRIANGLES {
                    return Err(TOO_MANY_TRIS.into());
                }
                *total_tris += 1;
                doc.tris.push([id_attr(&t, "v1")?, id_attr(&t, "v2")?, id_attr(&t, "v3")?]);
            }
            (b"component", false) => {
                if let Some((_, obj)) = cur.as_mut() {
                    if obj.comps.len() >= 100_000 {
                        return Err("Too many parts in the 3MF".into());
                    }
                    let m = match t.attr("transform") {
                        Some(s) => parse_transform(s)?,
                        None => Affine::IDENTITY,
                    };
                    obj.comps.push(Comp { id: id_attr(&t, "objectid")?, path: t.attr("p:path").map(str::to_string), m });
                }
            }
            (b"build", false) => in_build = !t.self_closing,
            (b"build", true) => in_build = false,
            (b"item", false) if in_build => {
                let m = match t.attr("transform") {
                    Some(s) => parse_transform(s)?,
                    None => Affine::IDENTITY,
                };
                if doc.build.len() >= 100_000 {
                    return Err("Too many placements in the 3MF".into());
                }
                doc.build.push((id_attr(&t, "objectid")?, t.attr("p:path").map(str::to_string), m));
            }
            _ => {}
        }
    }
    if cur.is_some() {
        return Err(BAD_XML.into());
    }
    Ok(doc)
}

/// All model files of the package, unpacked on demand.
struct Package<'a> {
    zip: Zip<'a>,
    docs: HashMap<String, Doc>,
    xml_bytes: usize,
    verts: usize,
    tris: usize,
    visits: usize,
}

impl Package<'_> {
    fn load(&mut self, path: &str) -> Result<(), String> {
        let key = path.trim_start_matches('/').to_ascii_lowercase();
        if self.docs.contains_key(&key) {
            return Ok(());
        }
        if self.docs.len() >= MAX_DOCS {
            return Err("Too many model files in the 3MF".into());
        }
        let e = self.zip.find(path).ok_or_else(|| format!("The 3MF has no model file {path}"))?.clone();
        if self.xml_bytes + e.size() > MAX_TOTAL_XML {
            return Err(crate::import_inflate::InflateError::TooBig.to_string());
        }
        self.xml_bytes += e.size();
        let xml = self.zip.read(&e, MAX_MODEL_BYTES)?;
        let doc = parse_doc(&xml, &mut self.verts, &mut self.tris)?;
        self.docs.insert(key, doc);
        Ok(())
    }

    fn emit(&mut self, file: &str, id: u32, m: &Affine, depth: usize, out: &mut ImportedMesh) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("The 3MF parts are nested too deeply".into());
        }
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return Err("Too many parts in the 3MF".into());
        }
        self.load(file)?;
        let key = file.trim_start_matches('/').to_ascii_lowercase();
        // copy what is needed so that the package can be borrowed again for the components
        let (unit, comps, part) = {
            let Some(doc) = self.docs.get(&key) else { return Ok(()) };
            let Some(obj) = doc.objects.get(&id) else { return Err(format!("The 3MF has no shape with number {id}")) };
            if obj.skip {
                return Ok(());
            }
            let part = if obj.t.is_empty() {
                None
            } else {
                Some((
                    doc.verts.get(obj.v.clone()).map(<[_]>::to_vec).ok_or(BAD_INDEX)?,
                    doc.tris.get(obj.t.clone()).map(<[_]>::to_vec).ok_or(BAD_INDEX)?,
                ))
            };
            let comps: Vec<(u32, Option<String>, Affine)> = obj.comps.iter().map(|c| (c.id, c.path.clone(), c.m)).collect();
            (doc.unit_mm, comps, part)
        };
        if let Some((v, t)) = part {
            append_transformed(out, &v, &t, m)?;
        }
        for (cid, cpath, cm) in comps {
            let mut inner = m.after(&cm);
            let f = cpath.as_deref().unwrap_or(file);
            // another model file counts in its own unit: its numbers are turned into this file's unit first
            inner = inner.after(&self.unit_change(file, unit, f)?);
            self.emit(f, cid, &inner, depth + 1, out)?;
        }
        Ok(())
    }

    /// The scale that turns numbers written in the unit of file `to` into the unit `from_unit` of file `from`
    /// (the identity inside one file).
    fn unit_change(&mut self, from: &str, from_unit: f64, to: &str) -> Result<Affine, String> {
        let key = |p: &str| p.trim_start_matches('/').to_ascii_lowercase();
        if key(from) == key(to) {
            return Ok(Affine::IDENTITY);
        }
        self.load(to)?;
        let to_unit = self.docs.get(&key(to)).map_or(from_unit, |d| d.unit_mm);
        let r = if from_unit > 0.0 && to_unit.is_finite() { to_unit / from_unit } else { 1.0 };
        Ok(Affine([[r, 0.0, 0.0, 0.0], [0.0, r, 0.0, 0.0], [0.0, 0.0, r, 0.0]]))
    }
}

/// Find the main model file: the relationship in `_rels/.rels`, else `3D/3dmodel.model`, else any `.model`.
fn model_path(zip: &Zip) -> Result<String, String> {
    if let Some(rels) = zip.find("_rels/.rels")
        && let Ok(bytes) = zip.read(rels, 1 << 20)
    {
        let mut pos = 0;
        while let Ok(Some(t)) = next_tag(&bytes, &mut pos) {
            if t.name == b"Relationship"
                && !t.closing
                && t.attr("Type").is_some_and(|ty| ty.ends_with("/3dmodel"))
                && let Some(target) = t.attr("Target")
                && zip.find(target).is_some()
            {
                return Ok(target.to_string());
            }
        }
    }
    if zip.find("3D/3dmodel.model").is_some() {
        return Ok("3D/3dmodel.model".into());
    }
    zip.entries
        .iter()
        .find(|e| e.name.to_ascii_lowercase().ends_with(".model"))
        .map(|e| e.name.clone())
        .ok_or_else(|| "No model (3D/3dmodel.model) found in the 3MF".to_string())
}

/// Inspect base materials without affecting the safety-critical geometry reader.
/// A single base colour maps exactly to the editor's sRGB. Multiple palette entries
/// cannot be painted onto a single distance volume and are reported explicitly.
pub(crate) fn appearance_3mf(bytes: &[u8]) -> crate::import_mesh::AppearanceInfo {
    use crate::import_mesh::AppearanceInfo;
    let mut out = AppearanceInfo::default();
    let Ok(zip) = Zip::open(bytes) else { return out };
    let Ok(path) = model_path(&zip) else { return out };
    let Some(entry) = zip.find(&path) else { return out };
    let Ok(xml) = zip.read(entry, MAX_MODEL_BYTES) else { return out };
    let mut pos = 0;
    let mut groups: HashMap<u32, Vec<crate::model::Material>> = HashMap::new();
    let mut current_group: Option<u32> = None;
    // what each object asks for, which objects it is made of, and what the build places
    let mut refs: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    let mut parts: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut items: Vec<u32> = Vec::new();
    let mut current_object: Option<u32> = None;
    let mut unsupported = false;
    let mut textures = false;
    while let Ok(Some(t)) = next_tag(&xml, &mut pos) {
        if t.closing {
            if t.name == b"basematerials" || t.name == b"colorgroup" {
                current_group = None;
            }
            if t.name == b"object" {
                current_object = None;
            }
            continue;
        }
        match t.name {
            b"basematerials" | b"colorgroup" => {
                current_group = t.attr("id").and_then(|s| s.parse::<u32>().ok());
                if let Some(id) = current_group {
                    groups.entry(id).or_default();
                }
                if t.self_closing {
                    current_group = None;
                }
            }
            b"base" | b"color" if current_group.is_some() => {
                let value = t.attr("displaycolor").or_else(|| t.attr("color"));
                if let Some(m) = value.and_then(crate::materials::from_3mf_hex) {
                    if let Some(g) = current_group.and_then(|id| groups.get_mut(&id)) {
                        if g.len() < 65536 {
                            g.push(m);
                        } else {
                            unsupported = true;
                        }
                    }
                } else {
                    unsupported = true;
                }
            }
            b"object" => {
                let id = t.attr("id").and_then(|s| s.parse::<u32>().ok());
                current_object = if t.self_closing { None } else { id };
                if let (Some(id), Some(pid), Some(idx)) =
                    (id, t.attr("pid").and_then(|s| s.parse::<u32>().ok()), t.attr("pindex").and_then(|s| s.parse::<usize>().ok()))
                {
                    refs.entry(id).or_default().push((pid, idx));
                }
            }
            b"component" => {
                if let (Some(obj), Some(part)) = (current_object, t.attr("objectid").and_then(|s| s.parse::<u32>().ok())) {
                    parts.entry(obj).or_default().push(part);
                }
            }
            b"item" => {
                if let Some(id) = t.attr("objectid").and_then(|s| s.parse::<u32>().ok()) {
                    items.push(id);
                }
            }
            b"triangle" => {
                if let (Some(obj), Some(pid)) = (current_object, t.attr("pid").and_then(|s| s.parse::<u32>().ok())) {
                    let p1 = t.attr("p1").and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                    let list = refs.entry(obj).or_default();
                    list.push((pid, p1));
                    for key in ["p2", "p3"] {
                        if let Some(index) = t.attr(key).and_then(|s| s.parse::<usize>().ok()) {
                            list.push((pid, index));
                        }
                    }
                }
            }
            b"texture2d" | b"texture2dgroup" => textures = true,
            b"compositematerials" | b"multiproperties" => unsupported = true,
            _ => {}
        }
    }
    // Resources may be written after objects, so resolve the palette once parsing is complete.
    // Object defaults are used by triangles with no per-face override.
    // only the objects that are really placed count: the build items and what they are made of
    // (without a build section: the objects nobody else uses as a part)
    let mut roots = items;
    if roots.is_empty() {
        let used: std::collections::HashSet<u32> = parts.values().flatten().copied().collect();
        let mut all: Vec<u32> = refs.keys().chain(parts.keys()).copied().filter(|i| !used.contains(i)).collect();
        all.sort_unstable();
        all.dedup();
        roots = all;
    }
    let mut placed: Vec<u32> = Vec::new();
    while let Some(id) = roots.pop() {
        if placed.contains(&id) || placed.len() > 4096 {
            continue;
        }
        placed.push(id);
        roots.extend(parts.get(&id).into_iter().flatten().copied());
    }
    placed.sort_unstable();
    let palette_refs: Vec<(u32, usize)> = placed.iter().flat_map(|id| refs.get(id).into_iter().flatten().copied()).collect();
    let mut distinct: Vec<crate::model::Material> = Vec::new();
    for (pid, idx) in palette_refs {
        if let Some(m) = groups.get(&pid).and_then(|v| v.get(idx)).copied() {
            if !distinct.contains(&m) {
                distinct.push(m);
            }
        } else {
            unsupported = true;
        }
        if distinct.len() > 1 {
            out.varies = true;
        }
    }
    // Some files define exactly one base material without attaching it to the object.
    if distinct.is_empty()
        && groups.len() == 1
        && let Some(one) = groups.values().next().and_then(|a| (a.len() == 1).then(|| a[0]))
    {
        distinct.push(one);
    }
    out.material = distinct.first().copied();
    if unsupported {
        out.notes.push("The 3MF has unsupported or invalid colour/material settings. Some looks will not be kept".into());
    }
    if textures {
        out.notes.push("Image textures in the 3MF can't be loaded".into());
    }
    out
}

/// Read a 3MF file: every build item (or, without a build section, every object) becomes part of
/// one mesh, in millimetres.
pub fn read_3mf(bytes: &[u8]) -> Result<ImportedMesh, String> {
    let zip = Zip::open(bytes)?;
    let main = model_path(&zip)?;
    let mut pkg = Package { zip, docs: HashMap::new(), xml_bytes: 0, verts: 0, tris: 0, visits: 0 };
    pkg.load(&main)?;
    let main_key = main.trim_start_matches('/').to_ascii_lowercase();
    let (unit, build, mut all_ids) = {
        let doc = pkg.docs.get(&main_key).ok_or(CUT_OFF)?;
        let mut ids: Vec<u32> = doc.objects.iter().filter(|(_, o)| !o.skip).map(|(k, _)| *k).collect();
        ids.sort_unstable();
        (doc.unit_mm, doc.build.iter().map(|(i, p, m)| (*i, p.clone(), *m)).collect::<Vec<_>>(), ids)
    };
    let scale = Affine([[unit, 0.0, 0.0, 0.0], [0.0, unit, 0.0, 0.0], [0.0, 0.0, unit, 0.0]]);
    let mut out = ImportedMesh::default();
    if build.is_empty() {
        // no build section: take the objects nobody else uses as a component
        let used: std::collections::HashSet<u32> =
            pkg.docs.get(&main_key).map(|d| d.objects.values().flat_map(|o| o.comps.iter().map(|c| c.id)).collect()).unwrap_or_default();
        all_ids.retain(|i| !used.contains(i));
        for id in all_ids {
            pkg.emit(&main, id, &scale, 0, &mut out)?;
        }
    } else {
        for (id, path, m) in build {
            let f = path.as_deref().unwrap_or(&main).to_string();
            let inner = scale.after(&m).after(&pkg.unit_change(&main, unit, &f)?);
            pkg.emit(&f, id, &inner, 0, &mut out)?;
        }
    }
    if out.triangles.is_empty() {
        return Err(NO_SHAPE.into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_formats::{self, Units};
    use crate::import_inflate::testutil::zip;
    use crate::import_mesh::tests::{box_mesh, sphere_mesh, surface};

    fn model(unit: &str, body: &str) -> Vec<u8> {
        format!("<?xml version=\"1.0\"?>\n<model unit=\"{unit}\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">{body}</model>")
            .into_bytes()
    }

    fn pkg(xml: &[u8]) -> Vec<u8> {
        zip(&[("3D/3dmodel.model", xml)], false)
    }

    const TETRA: &str = "<mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"10\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"10\" z=\"0\"/><vertex x=\"0\" y=\"0\" z=\"10\"/></vertices>\
        <triangles><triangle v1=\"0\" v2=\"2\" v3=\"1\"/><triangle v1=\"0\" v2=\"1\" v3=\"3\"/><triangle v1=\"0\" v2=\"3\" v3=\"2\"/><triangle v1=\"1\" v2=\"2\" v3=\"3\"/></triangles></mesh>";

    fn vol(m: &ImportedMesh) -> f64 {
        m.signed_volume()
    }

    #[test]
    fn units_become_millimetres() {
        for (u, f) in [("millimeter", 1.0), ("centimeter", 10.0), ("inch", 25.4), ("meter", 1000.0), ("micron", 0.001), ("foot", 304.8), ("", 1.0)] {
            let xml = model(u, &format!("<resources><object id=\"1\">{TETRA}</object></resources><build><item objectid=\"1\"/></build>"));
            let m = read_3mf(&pkg(&xml)).unwrap();
            let want = 1000.0 / 6.0 * f * f * f;
            assert!((vol(&m) - want).abs() / want < 1e-4, "{u}: {} vs {want}", vol(&m));
        }
    }

    #[test]
    fn build_items_transforms_and_components() {
        // object 2 = a tetra moved by a component transform; object 3 = a component of 2 twice;
        // the build item moves it again and mirrors x (negative determinant: the winding is fixed)
        let xml = model(
            "millimeter",
            &format!(
                "<resources><object id=\"1\">{TETRA}</object>\
                 <object id=\"2\" type=\"model\"><components><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 100 0 0\"/><component objectid=\"1\"/></components></object>\
                 <object id=\"9\" type=\"support\">{TETRA}</object></resources>\
                 <build><item objectid=\"2\" transform=\"-1 0 0 0 1 0 0 0 1 0 5 0\"/><item objectid=\"9\"/></build>"
            ),
        );
        let m = read_3mf(&pkg(&xml)).unwrap();
        assert_eq!(m.triangles.len(), 8);
        let v = 1000.0 / 6.0;
        assert!((vol(&m) - 2.0 * v).abs() < 1e-3, "{}", vol(&m));
        let (lo, hi) = m.bounds().unwrap();
        assert_eq!((lo[0], hi[0]), (-110.0, 0.0));
        assert_eq!((lo[1], hi[1]), (5.0, 15.0));
        // a rotation by 90 degrees about z (row vectors): x -> y
        let xml = model(
            "millimeter",
            &format!(
                "<resources><object id=\"1\">{TETRA}</object></resources><build><item objectid=\"1\" transform=\"0 1 0 -1 0 0 0 0 1 0 0 0\"/></build>"
            ),
        );
        let m = read_3mf(&pkg(&xml)).unwrap();
        assert!((vol(&m) - 1000.0 / 6.0).abs() < 1e-3);
        let (lo, hi) = m.bounds().unwrap();
        assert_eq!((lo[0], hi[0], lo[1], hi[1]), (-10.0, 0.0, 0.0, 10.0));
    }

    #[test]
    fn without_a_build_section_the_free_objects_are_used() {
        let xml = model(
            "millimeter",
            &format!(
                "<resources><object id=\"1\">{TETRA}</object><object id=\"2\"><components><component objectid=\"1\"/></components></object></resources>"
            ),
        );
        assert_eq!(read_3mf(&pkg(&xml)).unwrap().triangles.len(), 4);
    }

    #[test]
    fn objects_in_other_model_files_are_followed() {
        let main = model(
            "millimeter",
            "<resources><object id=\"5\"><components><component objectid=\"1\" p:path=\"/3D/Objects/part.model\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\"/></components></object></resources><build><item objectid=\"5\"/></build>",
        );
        let part = model("millimeter", &format!("<resources><object id=\"1\">{TETRA}</object></resources>"));
        let z = zip(&[("3D/3dmodel.model", &main), ("3D/Objects/part.model", &part)], true);
        assert_eq!(read_3mf(&z).unwrap().triangles.len(), 4);
        // the referenced file is missing
        let z = zip(&[("3D/3dmodel.model", &main)], false);
        assert!(read_3mf(&z).is_err());
    }

    #[test]
    fn a_model_file_in_another_unit_keeps_its_real_size() {
        // the main file counts in millimetres, the part file in centimetres: its 10-wide tetrahedron is 100 mm
        let main = model(
            "millimeter",
            "<resources><object id=\"5\"><components><component objectid=\"1\" p:path=\"/3D/Objects/part.model\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\"/></components></object></resources><build><item objectid=\"5\"/></build>",
        );
        let part = model("centimeter", &format!("<resources><object id=\"1\">{TETRA}</object></resources>"));
        let z = zip(&[("3D/3dmodel.model", &main), ("3D/Objects/part.model", &part)], true);
        let m = read_3mf(&z).unwrap();
        let want = 1000.0 / 6.0 * 1000.0;
        assert!((vol(&m) - want).abs() / want < 1e-4, "{} vs {want}", vol(&m));
    }

    #[test]
    fn an_object_that_is_not_placed_does_not_colour_the_shape() {
        let body = format!(
            "<resources><basematerials id=\"9\"><base name=\"red\" displaycolor=\"#FF0000\"/><base name=\"blue\" displaycolor=\"#0000FF\"/></basematerials>\
             <object id=\"1\" pid=\"9\" pindex=\"0\">{TETRA}</object><object id=\"2\" pid=\"9\" pindex=\"1\">{TETRA}</object></resources><build><item objectid=\"2\"/></build>"
        );
        let a = appearance_3mf(&pkg(&model("millimeter", &body)));
        let c = a.material.expect("a colour").color;
        assert!(c[2] > 0.5 && c[0] < 0.5, "blue expected, got {c:?}");
        assert!(!a.varies, "the unplaced red object must not make it two-coloured");
    }

    #[test]
    fn the_rels_file_names_the_model() {
        let xml = model("millimeter", &format!("<resources><object id=\"1\">{TETRA}</object></resources><build><item objectid=\"1\"/></build>"));
        let rels = b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/Other/m.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>";
        let z = zip(&[("_rels/.rels", rels), ("Other/m.model", &xml)], false);
        assert_eq!(read_3mf(&z).unwrap().triangles.len(), 4);
        // no rels: any .model file
        let z = zip(&[("Weird/x.MODEL", &xml)], false);
        assert_eq!(read_3mf(&z).unwrap().triangles.len(), 4);
    }

    #[test]
    fn xml_oddities_are_fine() {
        let xml = format!(
            "<?xml version=\"1.0\"?><!DOCTYPE model [<!ENTITY a \"b\">]><!-- c --><m:model xmlns:m=\"x\" unit='millimeter'><metadata name=\"a>b\">&amp;</metadata>\
             <m:resources><m:object id = '1' >{}</m:object></m:resources><build><item objectid=\"1\" /></build></m:model>",
            TETRA.replace("<vertex", "<m:vertex").replace("<triangle ", "<m:triangle ")
        );
        assert_eq!(read_3mf(&pkg(xml.as_bytes())).unwrap().triangles.len(), 4);
    }

    #[test]
    fn hostile_models_fail_cleanly() {
        let obj = |inner: &str| {
            model("millimeter", &format!("<resources><object id=\"1\">{inner}</object></resources><build><item objectid=\"1\"/></build>"))
        };
        for bad in [
            // index out of range
            obj(&TETRA.replace("v3=\"3\"", "v3=\"4\"")),
            obj(&TETRA.replace("v3=\"3\"", "v3=\"-1\"")),
            obj(&TETRA.replace("z=\"10\"", "z=\"nan\"")),
            obj(&TETRA.replace("z=\"10\"", "z=\"1e999\"")),
            obj(&TETRA.replace("y=\"10\"", "y=\"abc\"")),
            obj(&TETRA.replace("y=\"10\"", "")),
            // empty mesh, unknown item, missing end tags
            obj("<mesh><vertices/><triangles/></mesh>"),
            model("millimeter", "<resources/><build><item objectid=\"7\"/></build>"),
            model("millimeter", "<resources><object id=\"1\"><mesh>"),
            model("millimeter", "<resources><object id=\"x\"/></resources>"),
            b"<model><!-- never closed".to_vec(),
            b"<model><resources><object id=\"1\" ".to_vec(),
            Vec::new(),
        ] {
            assert!(read_3mf(&pkg(&bad)).is_err(), "{}", String::from_utf8_lossy(&bad));
        }
        // a bad transform
        for tr in ["1 2 3", "1 0 0 0 1 0 0 0 1 0 0 nan", "a b c d e f g h i j k l", "1 0 0 0 1 0 0 0 1 0 0 1e99"] {
            let xml = model(
                "millimeter",
                &format!("<resources><object id=\"1\">{TETRA}</object></resources><build><item objectid=\"1\" transform=\"{tr}\"/></build>"),
            );
            assert!(read_3mf(&pkg(&xml)).is_err(), "{tr}");
        }
        // a component cycle and a very deep chain
        let cyc = model(
            "millimeter",
            "<resources><object id=\"1\"><components><component objectid=\"2\"/></components></object><object id=\"2\"><components><component objectid=\"1\"/></components></object></resources><build><item objectid=\"1\"/></build>",
        );
        assert!(read_3mf(&pkg(&cyc)).is_err());
        // exponential fan-out: 10 components each referencing the next level, 10 levels
        let mut body = String::from("<resources><object id=\"0\">");
        body.push_str(&format!("{TETRA}</object>"));
        for lvl in 1..=10 {
            body.push_str(&format!("<object id=\"{lvl}\"><components>"));
            for _ in 0..10 {
                body.push_str(&format!("<component objectid=\"{}\"/>", lvl - 1));
            }
            body.push_str("</components></object>");
        }
        body.push_str("</resources><build><item objectid=\"10\"/></build>");
        assert!(read_3mf(&pkg(&model("millimeter", &body))).is_err());
        // not a zip, a zip without a model
        assert!(read_3mf(b"hello").is_err());
        assert!(read_3mf(&zip(&[("a.txt", b"x")], false)).is_err());
    }

    #[test]
    fn a_zip_bomb_model_is_refused_without_eating_the_memory() {
        // 10 MB of spaces deflate to a few KB; the declared size is above the (test) cap
        let big = vec![b' '; 10 * 1024 * 1024];
        let z = zip(&[("3D/3dmodel.model", &big)], true);
        let t = std::time::Instant::now();
        assert!(read_3mf(&z).is_err());
        assert!(t.elapsed().as_secs() < 20);
    }

    #[test]
    fn corrupt_packages_never_panic() {
        let z = export_formats::threemf(&surface(&box_mesh([0.0; 3], [1.0; 3])), "b", Units::Cm).unwrap();
        assert!(read_3mf(&z).is_ok());
        for cut in (0..z.len()).step_by(3) {
            let _ = read_3mf(&z[..cut]);
        }
        for i in (0..z.len()).step_by(5) {
            let mut c = z.clone();
            c[i] ^= 0xFF;
            let _ = read_3mf(&c);
        }
    }

    #[test]
    fn our_own_3mf_writer_round_trips_in_every_unit() {
        let s = sphere_mesh(10.0, 12, 24);
        let v = s.signed_volume();
        for u in Units::ALL {
            let mut scaled = s.clone();
            scaled.scale(u.per_mm() as f32);
            let z = export_formats::threemf(&surface(&scaled), "s", u).unwrap();
            let m = read_3mf(&z).unwrap();
            assert_eq!(m.triangles.len(), s.triangles.len());
            assert!((vol(&m) - v).abs() / v < 1e-3, "{u:?}: {} vs {v}", vol(&m));
        }
    }
    #[test]
    fn basematerials_and_multicolour_are_reported() {
        let mesh = TETRA;
        let prefix = "<resources><basematerials id=\"9\"><base name=\"red\" displaycolor=\"#FF0000\"/><base name=\"blue\" displaycolor=\"#0000FF\"/></basematerials>";
        let xml = model(
            "millimeter",
            &format!("{prefix}<object id=\"1\" pid=\"9\" pindex=\"0\">{mesh}</object></resources><build><item objectid=\"1\"/></build>"),
        );
        let a = appearance_3mf(&pkg(&xml));
        assert_eq!(a.material.unwrap().color, [1.0, 0.0, 0.0]);
        assert!(!a.varies);
        let xml = model(
            "millimeter",
            &format!(
                "{prefix}<object id=\"1\" pid=\"9\" pindex=\"0\">{}</object></resources><build><item objectid=\"1\"/></build>",
                mesh.replacen("<triangle v1=", "<triangle pid=\"9\" p1=\"1\" v1=", 1)
            ),
        );
        let a = appearance_3mf(&pkg(&xml));
        assert!(a.varies);
        let s = surface(&box_mesh([0.0; 3], [1.0; 3]));
        let z = export_formats::threemf(&s, "roundtrip", Units::Mm).unwrap();
        let m = appearance_3mf(&z).material.unwrap();
        assert!((m.color[0] - 200.0 / 255.0).abs() < 0.002);
    }
}
