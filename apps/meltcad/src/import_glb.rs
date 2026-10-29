//! GLB (binary glTF 2.0) import: the POSITION and the indices of every triangle primitive of every
//! mesh, placed with the node transforms (matrix or translation/rotation/scale) of the scene.
//! Float and (normalised) integer positions are understood, triangle lists, strips and fans.
//! Not understood (and refused with a friendly message): compressed meshes (Draco, meshopt),
//! sparse accessors and buffers stored outside the file.

use serde_json::Value;

use crate::import_mesh::{
    Affine, BAD_INDEX, BAD_NUMBER, CUT_OFF, ImportedMesh, MAX_TRIANGLES, MAX_VERTICES, NO_SHAPE, TOO_MANY_TRIS, TOO_MANY_VERTS, append_transformed,
};

/// The meshes that `read_glb` would place: those of the nodes of the active scene (or, with no
/// scene, of the root nodes; with no nodes at all, every mesh).
fn reachable_meshes(json: &Value) -> Vec<usize> {
    let nodes = json.get("nodes").and_then(Value::as_array);
    let roots: Vec<usize> =
        match json.get("scenes").and_then(|s| s.get(usz(json.get("scene")).unwrap_or(0))).and_then(|s| s.get("nodes")).and_then(Value::as_array) {
            Some(a) => a.iter().filter_map(|v| usz(Some(v))).collect(),
            None => match nodes {
                Some(nodes) => {
                    let mut is_child = vec![false; nodes.len()];
                    for n in nodes {
                        for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
                            if let Some(slot) = usz(Some(c)).and_then(|i| is_child.get_mut(i)) {
                                *slot = true;
                            }
                        }
                    }
                    (0..nodes.len()).filter(|i| !is_child[*i]).collect()
                }
                None => return (0..json.get("meshes").and_then(Value::as_array).map_or(0, Vec::len)).collect(),
            },
        };
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
    let mut visits = 0usize;
    while let Some((i, depth)) = stack.pop() {
        visits += 1;
        if visits > MAX_NODE_VISITS || depth > MAX_DEPTH {
            break;
        }
        let Some(n) = nodes.and_then(|a| a.get(i)) else { continue };
        if let Some(m) = usz(n.get("mesh"))
            && !out.contains(&m)
        {
            out.push(m);
        }
        for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
            if let Some(c) = usz(Some(c)) {
                stack.push((c, depth + 1));
            }
        }
    }
    out.sort_unstable();
    out
}

/// Read only appearance metadata; never changes geometry decoding or allocates image pixels.
pub(crate) fn appearance_glb(bytes: &[u8]) -> crate::import_mesh::AppearanceInfo {
    use crate::import_mesh::AppearanceInfo;
    let mut out = AppearanceInfo::default();
    if bytes.get(..4) != Some(b"glTF") {
        return out;
    }
    let mut offset = 12usize;
    let json_bytes = loop {
        let Some(length) = u32at(bytes, offset).map(|n| n as usize) else { return out };
        let Some(kind) = u32at(bytes, offset + 4) else { return out };
        let Some(start) = offset.checked_add(8) else { return out };
        let Some(end) = start.checked_add(length) else { return out };
        let Some(chunk) = bytes.get(start..end) else { return out };
        if kind == 0x4E4F_534A {
            break chunk;
        }
        let Some(next) = start.checked_add(length.div_ceil(4) * 4) else { return out };
        if next <= offset || next >= bytes.len() {
            return out;
        }
        offset = next;
    };
    let Ok(doc) = serde_json::from_slice::<Value>(json_bytes) else { return out };
    let materials = doc.get("materials").and_then(Value::as_array);
    let mut used: Vec<usize> = Vec::new();
    let mut vertex_colours = false;
    if let Some(meshes) = doc.get("meshes").and_then(Value::as_array) {
        // only the meshes the model really takes from the file (the active scene), as read_glb does
        let reached = reachable_meshes(&doc);
        for mesh in reached.iter().filter_map(|i| meshes.get(*i)) {
            if let Some(primitives) = mesh.get("primitives").and_then(Value::as_array) {
                for p in primitives {
                    if p.get("mode").and_then(Value::as_u64).unwrap_or(4) < 4 {
                        continue;
                    }
                    if p.get("attributes").and_then(|a| a.get("COLOR_0")).is_some() {
                        vertex_colours = true;
                    }
                    let idx = usz(p.get("material")).unwrap_or(usize::MAX);
                    if !used.contains(&idx) {
                        used.push(idx);
                    }
                }
            }
        }
    }
    if used.len() > 1 || vertex_colours {
        out.varies = true;
    }
    for idx in used {
        let Some(mat) = materials.and_then(|a| a.get(idx)) else { continue };
        let pbr = mat.get("pbrMetallicRoughness");
        let v = pbr.and_then(|p| p.get("baseColorFactor")).and_then(Value::as_array);
        let mut rgba = [1.0f32; 4];
        if let Some(a) = v
            && a.len() == 4
        {
            for (i, slot) in rgba.iter_mut().enumerate() {
                let Some(x) = a[i].as_f64() else { return out };
                *slot = x as f32;
            }
        }
        let rough = pbr.and_then(|p| p.get("roughnessFactor")).and_then(Value::as_f64).unwrap_or(1.0) as f32;
        let metal = pbr.and_then(|p| p.get("metallicFactor")).and_then(Value::as_f64).unwrap_or(1.0) as f32;
        if out.material.is_none() {
            out.material = crate::materials::from_gltf_pbr(rgba, rough, metal);
        }
        if pbr.and_then(|p| p.get("baseColorTexture")).is_some()
            || mat.get("normalTexture").is_some()
            || mat.get("occlusionTexture").is_some()
            || mat.get("emissiveTexture").is_some()
        {
            out.notes.push("GLB image textures (base colour, normals, etc.) can't be loaded. A plain colour is used instead".into());
        }
        if mat.get("extensions").is_some() {
            out.notes.push("GLB extended materials can't be fully reproduced".into());
        }
        if rgba[3] < 1.0 || mat.get("alphaMode").and_then(Value::as_str).is_some_and(|a| a != "OPAQUE") {
            out.notes.push("GLB transparency is approximated as glass (not the same as alpha blending)".into());
        }
        if mat.get("emissiveFactor").is_some() {
            out.notes.push("GLB glow colours are not reproduced".into());
        }
    }
    if vertex_colours {
        out.notes.push("GLB vertex colours (COLOR_0) can't be turned into a single-colour SDF".into());
    }
    if out.material.is_none() && vertex_colours {
        out.notes.push("GLB has only vertex colours, so the default material is used".into());
    }
    out
}

const BAD_GLB: &str = "The GLB file is broken";
const MAX_NODE_VISITS: usize = 100_000;
const MAX_DEPTH: usize = 64;

fn u32at(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn usz(v: Option<&Value>) -> Option<usize> {
    v?.as_u64().and_then(|x| usize::try_from(x).ok())
}

/// One accessor, checked against the binary chunk.
struct Accessor {
    count: usize,
    ncomp: usize,
    ctype: u64,
    normalized: bool,
    offset: usize,
    stride: usize,
}

impl Accessor {
    fn comp_size(ctype: u64) -> Option<usize> {
        match ctype {
            5120 | 5121 => Some(1),
            5122 | 5123 => Some(2),
            5125 | 5126 => Some(4),
            _ => None,
        }
    }

    fn open(json: &Value, bin: &[u8], index: usize) -> Result<Accessor, String> {
        let a = json.get("accessors").and_then(|x| x.get(index)).ok_or(BAD_GLB)?;
        if a.get("sparse").is_some() {
            return Err("Sparse GLB data is not supported".into());
        }
        let count = usz(a.get("count")).ok_or(BAD_GLB)?;
        if count > MAX_VERTICES.max(MAX_TRIANGLES * 3) {
            return Err(TOO_MANY_VERTS.into());
        }
        let ncomp = match a.get("type").and_then(Value::as_str) {
            Some("SCALAR") => 1,
            Some("VEC3") => 3,
            Some("VEC2") => 2,
            Some("VEC4") => 4,
            _ => return Err(BAD_GLB.into()),
        };
        let ctype = a.get("componentType").and_then(Value::as_u64).ok_or(BAD_GLB)?;
        let csize = Self::comp_size(ctype).ok_or(BAD_GLB)?;
        let view = json.get("bufferViews").and_then(|x| x.get(usz(a.get("bufferView")).unwrap_or(usize::MAX))).ok_or(BAD_GLB)?;
        if usz(view.get("buffer")).unwrap_or(0) != 0 {
            return Err("Data stored outside the GLB is not supported".into());
        }
        let view_off = usz(view.get("byteOffset")).unwrap_or(0);
        let view_len = usz(view.get("byteLength")).ok_or(BAD_GLB)?;
        if view_off.checked_add(view_len).is_none_or(|e| e > bin.len()) {
            return Err(CUT_OFF.into());
        }
        let elem = csize.checked_mul(ncomp).ok_or(BAD_GLB)?;
        let stride = usz(view.get("byteStride")).filter(|s| *s != 0).unwrap_or(elem);
        if stride < elem || stride > 252 {
            return Err(BAD_GLB.into());
        }
        let view_end = view_off.checked_add(view_len).ok_or(CUT_OFF)?;
        let offset = view_off.checked_add(usz(a.get("byteOffset")).unwrap_or(0)).ok_or(CUT_OFF)?;
        // A wrapped offset may land *before* the view's beginning; checking
        // only the last byte against the upper bound is insufficient.
        if offset < view_off || offset > view_end {
            return Err(CUT_OFF.into());
        }
        if count > 0 {
            let last = (count - 1).checked_mul(stride).and_then(|x| x.checked_add(offset)).and_then(|x| x.checked_add(elem)).ok_or(BAD_GLB)?;
            if last > view_end {
                return Err(CUT_OFF.into());
            }
        }
        Ok(Accessor { count, ncomp, ctype, normalized: a.get("normalized").and_then(Value::as_bool).unwrap_or(false), offset, stride })
    }

    /// Component `c` of element `i` as a number (normalised integers map to 0..1 or -1..1).
    fn get(&self, bin: &[u8], i: usize, c: usize) -> Option<f64> {
        if i >= self.count || c >= self.ncomp {
            return None;
        }
        let csize = Self::comp_size(self.ctype)?;
        let o = self.offset.checked_add(i.checked_mul(self.stride)?)?.checked_add(c.checked_mul(csize)?)?;
        let b = bin.get(o..o.checked_add(csize)?)?;
        let v = match self.ctype {
            5120 => {
                let x = f64::from(b[0] as i8);
                if self.normalized { (x / 127.0).max(-1.0) } else { x }
            }
            5121 => {
                let x = f64::from(b[0]);
                if self.normalized { x / 255.0 } else { x }
            }
            5122 => {
                let x = f64::from(i16::from_le_bytes([b[0], b[1]]));
                if self.normalized { (x / 32767.0).max(-1.0) } else { x }
            }
            5123 => {
                let x = f64::from(u16::from_le_bytes([b[0], b[1]]));
                if self.normalized { x / 65535.0 } else { x }
            }
            5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        };
        Some(v)
    }
}

/// glTF matrices are column-major.
fn node_matrix(n: &Value) -> Result<Affine, String> {
    let nums = |key: &str, len: usize| -> Result<Option<Vec<f64>>, String> {
        match n.get(key) {
            None => Ok(None),
            Some(Value::Array(a)) if a.len() == len => {
                let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                if v.len() == len && v.iter().all(|x| x.is_finite() && x.abs() < 1.0e12) { Ok(Some(v)) } else { Err(BAD_NUMBER.into()) }
            }
            Some(_) => Err(BAD_GLB.into()),
        }
    };
    if let Some(m) = nums("matrix", 16)? {
        let g = |r: usize, c: usize| m.get(c * 4 + r).copied().unwrap_or(0.0);
        return Ok(Affine([[g(0, 0), g(0, 1), g(0, 2), g(0, 3)], [g(1, 0), g(1, 1), g(1, 2), g(1, 3)], [g(2, 0), g(2, 1), g(2, 2), g(2, 3)]]));
    }
    let t = nums("translation", 3)?.unwrap_or_else(|| vec![0.0; 3]);
    let s = nums("scale", 3)?.unwrap_or_else(|| vec![1.0; 3]);
    let q = nums("rotation", 4)?.unwrap_or_else(|| vec![0.0, 0.0, 0.0, 1.0]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let len = (x * x + y * y + z * z + w * w).sqrt();
    if len < 1e-12 {
        return Err(BAD_NUMBER.into());
    }
    let (x, y, z, w) = (x / len, y / len, z / len, w / len);
    let r = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ];
    Ok(Affine([
        [r[0][0] * s[0], r[0][1] * s[1], r[0][2] * s[2], t[0]],
        [r[1][0] * s[0], r[1][1] * s[1], r[1][2] * s[2], t[1]],
        [r[2][0] * s[0], r[2][1] * s[1], r[2][2] * s[2], t[2]],
    ]))
}

/// The triangles of one primitive and its points.
/// The points and the triangles of one primitive.
type Part = (Vec<[f32; 3]>, Vec<[u32; 3]>);

fn read_primitive(json: &Value, bin: &[u8], prim: &Value) -> Result<Option<Part>, String> {
    let mode = prim.get("mode").and_then(Value::as_u64).unwrap_or(4);
    if !(4..=6).contains(&mode) {
        return Ok(None); // points and lines
    }
    if prim.get("extensions").is_some_and(|e| e.get("KHR_draco_mesh_compression").is_some() || e.get("EXT_meshopt_compression").is_some()) {
        return Err("Compressed GLB (Draco etc.) is not supported".into());
    }
    let Some(pos_index) = usz(prim.get("attributes").and_then(|a| a.get("POSITION"))) else { return Ok(None) };
    let acc = Accessor::open(json, bin, pos_index)?;
    if acc.ncomp != 3 {
        return Err(BAD_GLB.into());
    }
    let mut positions = Vec::with_capacity(acc.count);
    for i in 0..acc.count {
        let mut p = [0f32; 3];
        for (c, slot) in p.iter_mut().enumerate() {
            let v = acc.get(bin, i, c).ok_or(CUT_OFF)?;
            if !v.is_finite() || v.abs() > f64::from(crate::import_mesh::MAX_COORD) {
                return Err(BAD_NUMBER.into());
            }
            *slot = v as f32;
        }
        positions.push(p);
    }
    let indices: Vec<u32> = match usz(prim.get("indices")) {
        Some(ii) => {
            let ia = Accessor::open(json, bin, ii)?;
            if ia.ncomp != 1 || ia.ctype == 5126 || ia.normalized {
                return Err(BAD_GLB.into());
            }
            let mut v = Vec::with_capacity(ia.count);
            for i in 0..ia.count {
                let x = ia.get(bin, i, 0).ok_or(CUT_OFF)?;
                if x < 0.0 || x >= positions.len() as f64 {
                    return Err(BAD_INDEX.into());
                }
                v.push(x as u32);
            }
            v
        }
        None => (0..positions.len() as u32).collect(),
    };
    let mut tris: Vec<[u32; 3]> = Vec::new();
    match mode {
        4 => {
            tris.reserve(indices.len() / 3);
            for t in indices.as_chunks::<3>().0.iter() {
                tris.push([t[0], t[1], t[2]]);
            }
        }
        5 => {
            for k in 2..indices.len() {
                if let (Some(&a), Some(&b), Some(&c)) = (indices.get(k - 2), indices.get(k - 1), indices.get(k)) {
                    tris.push(if k % 2 == 0 { [a, b, c] } else { [b, a, c] });
                }
            }
        }
        _ => {
            for k in 2..indices.len() {
                if let (Some(&a), Some(&b), Some(&c)) = (indices.first(), indices.get(k - 1), indices.get(k)) {
                    tris.push([a, b, c]);
                }
            }
        }
    }
    if tris.len() > MAX_TRIANGLES {
        return Err(TOO_MANY_TRIS.into());
    }
    Ok(Some((positions, tris)))
}

struct Walk<'a> {
    json: &'a Value,
    bin: &'a [u8],
    visits: usize,
    out: ImportedMesh,
}

impl Walk<'_> {
    fn mesh(&mut self, index: usize, m: &Affine) -> Result<(), String> {
        let Some(prims) = self.json.get("meshes").and_then(|x| x.get(index)).and_then(|x| x.get("primitives")).and_then(Value::as_array) else {
            return Ok(());
        };
        for p in prims {
            if let Some((pos, tris)) = read_primitive(self.json, self.bin, p)? {
                append_transformed(&mut self.out, &pos, &tris, m)?;
            }
        }
        Ok(())
    }

    fn node(&mut self, index: usize, parent: &Affine, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("The GLB nodes are nested too deeply".into());
        }
        self.visits += 1;
        if self.visits > MAX_NODE_VISITS {
            return Err("Too many nodes in the GLB".into());
        }
        let Some(n) = self.json.get("nodes").and_then(|x| x.get(index)) else { return Err(BAD_GLB.into()) };
        let m = parent.after(&node_matrix(n)?);
        if let Some(mi) = usz(n.get("mesh")) {
            self.mesh(mi, &m)?;
        }
        if let Some(children) = n.get("children").and_then(Value::as_array) {
            for c in children {
                self.node(usz(Some(c)).ok_or(BAD_GLB)?, &m, depth + 1)?;
            }
        }
        Ok(())
    }
}

/// Read a GLB file. glTF is metres by definition: the numbers are turned into millimetres (and
/// our own exporter writes metres, so a part keeps its real size in other programs).
pub fn read_glb(b: &[u8]) -> Result<ImportedMesh, String> {
    if b.get(..4) != Some(b"glTF") {
        return Err(BAD_GLB.into());
    }
    if u32at(b, 4) != Some(2) {
        return Err("Only glTF 2.0 GLB is supported".into());
    }
    let total = u32at(b, 8).ok_or(BAD_GLB)? as usize;
    if total > b.len() || total < 12 {
        return Err(CUT_OFF.into());
    }
    let mut json_chunk: Option<&[u8]> = None;
    let mut bin: &[u8] = &[];
    let mut seen_bin = false;
    let mut o = 12usize;
    while o + 8 <= total {
        let len = u32at(b, o).ok_or(BAD_GLB)? as usize;
        let kind = u32at(b, o + 4).ok_or(BAD_GLB)?;
        let body = b.get(o + 8..o.checked_add(8).and_then(|x| x.checked_add(len)).ok_or(BAD_GLB)?).ok_or(CUT_OFF)?;
        if o + 8 + len > total {
            return Err(CUT_OFF.into());
        }
        match kind {
            0x4E4F_534A if json_chunk.is_none() => json_chunk = Some(body),
            0x004E_4942 if !seen_bin => {
                bin = body;
                seen_bin = true;
            }
            _ => {}
        }
        o += 8 + len.div_ceil(4) * 4;
    }
    let json: Value = serde_json::from_slice(json_chunk.ok_or(BAD_GLB)?).map_err(|_| BAD_GLB.to_string())?;
    if let Some(req) = json.get("extensionsRequired").and_then(Value::as_array)
        && req.iter().filter_map(Value::as_str).any(|e| matches!(e, "KHR_draco_mesh_compression" | "EXT_meshopt_compression"))
    {
        return Err("Compressed GLB (Draco etc.) is not supported".into());
    }
    if let Some(buffers) = json.get("buffers").and_then(Value::as_array)
        && (buffers.len() > 1 || buffers.first().is_some_and(|b0| b0.get("uri").is_some()))
    {
        return Err("Data stored outside the GLB is not supported".into());
    }
    let mut w = Walk { json: &json, bin, visits: 0, out: ImportedMesh::default() };
    let scene_nodes: Option<Vec<usize>> = {
        let scene = usz(json.get("scene")).unwrap_or(0);
        json.get("scenes")
            .and_then(|s| s.get(scene))
            .and_then(|s| s.get("nodes"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| usz(Some(v))).collect())
    };
    match scene_nodes {
        Some(nodes) => {
            for n in nodes {
                w.node(n, &Affine::IDENTITY, 0)?;
            }
        }
        None => {
            // no scene: the root nodes (nobody's child), or every mesh as it is
            let nodes = json.get("nodes").and_then(Value::as_array);
            if let Some(nodes) = nodes {
                let mut is_child = vec![false; nodes.len()];
                for n in nodes {
                    for c in n.get("children").and_then(Value::as_array).into_iter().flatten() {
                        if let Some(slot) = usz(Some(c)).and_then(|i| is_child.get_mut(i)) {
                            *slot = true;
                        }
                    }
                }
                for (i, child) in is_child.iter().enumerate() {
                    if !child {
                        w.node(i, &Affine::IDENTITY, 0)?;
                    }
                }
            } else {
                let count = json.get("meshes").and_then(Value::as_array).map_or(0, Vec::len);
                for i in 0..count {
                    w.mesh(i, &Affine::IDENTITY)?;
                }
            }
        }
    }
    if w.out.triangles.is_empty() {
        return Err(NO_SHAPE.into());
    }
    // glTF coordinates are metres; the model is in millimetres
    for p in &mut w.out.positions {
        for c in p.iter_mut() {
            *c *= 1000.0;
            if !c.is_finite() || c.abs() > crate::import_mesh::MAX_COORD {
                return Err("The GLB is too large".into());
            }
        }
    }
    Ok(w.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_formats;
    use crate::import_mesh::tests::{box_mesh, sphere_mesh, surface};
    use serde_json::json;

    /// A GLB from a JSON document and a binary chunk.
    fn glb(doc: &Value, bin: &[u8]) -> Vec<u8> {
        let mut text = serde_json::to_vec(doc).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let mut b = bin.to_vec();
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        let total = 12 + 8 + text.len() + 8 + b.len();
        let mut out = b"glTF".to_vec();
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&text);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&b);
        out
    }

    /// A single right triangle (legs 2 and 3) with u16 indices, in bytes.
    fn tri_bin() -> Vec<u8> {
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        bin
    }

    fn tri_doc(nodes: Value, scene_nodes: Value) -> Value {
        json!({
            "asset": {"version": "2.0"},
            "scene": 0, "scenes": [{"nodes": scene_nodes}],
            "nodes": nodes,
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 6}],
            "buffers": [{"byteLength": 42}]
        })
    }

    #[test]
    fn accessor_offsets_cannot_wrap_or_escape_the_buffer_view() {
        let mut bin = vec![0u8; 20];
        bin[8..12].copy_from_slice(&1.25f32.to_le_bytes());
        let make = |offset: u64, count: usize| {
            json!({
                "accessors":[{"bufferView":0,"byteOffset":offset,"componentType":5126,"count":count,"type":"VEC3"}],
                "bufferViews":[{"buffer":0,"byteOffset":8,"byteLength":12}]
            })
        };
        let good = Accessor::open(&make(0, 1), &bin, 0).expect("valid accessor at view start");
        assert_eq!(good.get(&bin, 0, 0), Some(1.25));
        assert!(Accessor::open(&make(1, 1), &bin, 0).is_err(), "unaligned overrun is out of the view");
        assert!(Accessor::open(&make(usize::MAX as u64 - 7, 1), &bin, 0).is_err(), "checked sum must reject overflow");
        assert!(Accessor::open(&make(13, 0), &bin, 0).is_err(), "even an empty accessor may not point beyond the view");
        assert!(Accessor::open(&make(12, 0), &bin, 0).is_ok(), "empty accessor at the view end is allowed");
    }

    #[test]
    fn a_hand_made_triangle_with_node_transforms() {
        let bin = tri_bin();
        let m = read_glb(&glb(&tri_doc(json!([{"mesh": 0}]), json!([0])), &bin)).unwrap();
        assert_eq!((m.positions.len(), m.triangles.len()), (3, 1));
        assert!((m.surface_area() - 3.0e6).abs() < 10.0, "metres become millimetres");
        // translation + scale + rotation by 90 degrees about z (quaternion), parent and child
        let s = 0.5f64.sqrt();
        let nodes = json!([{"children": [1], "translation": [10.0, 0.0, 0.0], "scale": [2.0, 2.0, 2.0]}, {"mesh": 0, "rotation": [0.0, 0.0, s, s]}]);
        let m = read_glb(&glb(&tri_doc(nodes, json!([0])), &bin)).unwrap();
        assert!((m.surface_area() - 12.0e6).abs() < 500.0);
        let (lo, hi) = m.bounds().unwrap();
        // x' = 10 + 2*(-y): [4, 10]; y' = 2*x: [0, 4] (metres, read as millimetres)
        assert!((lo[0] - 4000.0).abs() < 0.1 && (hi[0] - 10000.0).abs() < 0.1, "{lo:?} {hi:?}");
        assert!(lo[1].abs() < 0.1 && (hi[1] - 4000.0).abs() < 0.1);
        // a matrix (column major): mirror x -> the winding is flipped so the area stays positive
        let nodes = json!([{"mesh": 0, "matrix": [-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0, 1.0]}]);
        let m = read_glb(&glb(&tri_doc(nodes, json!([0])), &bin)).unwrap();
        let (lo, hi) = m.bounds().unwrap();
        assert!((lo[0] - 3000.0).abs() < 0.1 && (hi[0] - 5000.0).abs() < 0.1);
        let t = m.triangles[0];
        let p = |i: u32| m.positions[i as usize];
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let z = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        assert!(z > 0.0, "still facing +z");
    }

    #[test]
    fn scenes_without_nodes_and_unindexed_primitives() {
        let bin = tri_bin();
        let mut doc = tri_doc(json!([]), json!([]));
        doc.as_object_mut().unwrap().remove("scenes");
        doc.as_object_mut().unwrap().remove("nodes");
        doc["meshes"][0]["primitives"][0].as_object_mut().unwrap().remove("indices");
        let m = read_glb(&glb(&doc, &bin)).unwrap();
        assert_eq!(m.triangles, vec![[0, 1, 2]]);
        // strips and fans
        let mut quad = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]] {
            for c in p {
                quad.extend_from_slice(&c.to_le_bytes());
            }
        }
        for mode in [5, 6] {
            let doc = json!({"asset": {"version": "2.0"}, "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "mode": mode}]}],
                "accessors": [{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3"}],
                "bufferViews": [{"buffer": 0, "byteLength": 48}], "buffers": [{"byteLength": 48}]});
            let m = read_glb(&glb(&doc, &quad)).unwrap();
            assert_eq!(m.triangles.len(), 2);
            assert!((m.surface_area() - 1.0e6).abs() < 10.0);
        }
    }

    #[test]
    fn normalised_integer_positions_and_strides() {
        // three i16 normalised positions with a 8 byte stride (padding after each)
        let mut bin = Vec::new();
        for p in [[0i16, 0, 0], [32767, 0, 0], [0, 32767, 0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
            bin.extend_from_slice(&[0, 0]);
        }
        let doc = json!({"asset": {"version": "2.0"}, "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{"bufferView": 0, "componentType": 5122, "normalized": true, "count": 3, "type": "VEC3"}],
            "bufferViews": [{"buffer": 0, "byteLength": 24, "byteStride": 8}], "buffers": [{"byteLength": 24}]});
        let m = read_glb(&glb(&doc, &bin)).unwrap();
        assert!((m.surface_area() - 0.5e6).abs() < 10.0);
    }

    #[test]
    fn the_colour_comes_from_the_mesh_that_is_placed_not_from_an_unused_one() {
        let bin = tri_bin();
        let mut doc = tri_doc(json!([{"mesh": 1}]), json!([0]));
        let prim = doc["meshes"][0]["primitives"][0].clone();
        let mut red = prim.clone();
        red["material"] = json!(0);
        let mut blue = prim;
        blue["material"] = json!(1);
        doc["meshes"] = json!([{"primitives": [red]}, {"primitives": [blue]}]);
        doc["materials"] = json!([
            {"pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}},
            {"pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.0, 1.0, 1.0]}}
        ]);
        let a = appearance_glb(&glb(&doc, &bin));
        let c = a.material.expect("a colour").color;
        assert!(c[2] > 0.5 && c[0] < 0.5, "blue expected, got {c:?}");
        assert!(!a.varies, "the unused red mesh must not make the file look multi-coloured");
    }

    #[test]
    fn a_one_metre_cube_is_a_thousand_millimetres() {
        let bin = tri_bin();
        let m = read_glb(&glb(&tri_doc(json!([{"mesh": 0}]), json!([0])), &bin)).unwrap();
        // tri_bin is 2 x 3 metres: the size is in millimetres
        let s = m.size();
        assert!(s.iter().any(|x| *x >= 999.0), "{s:?}");
    }

    #[test]
    fn hostile_glb_files() {
        let bin = tri_bin();
        let good = tri_doc(json!([{"mesh": 0}]), json!([0]));
        assert!(read_glb(&glb(&good, &bin)).is_ok());
        let edit = |f: &dyn Fn(&mut Value)| {
            let mut d = good.clone();
            f(&mut d);
            read_glb(&glb(&d, &bin))
        };
        assert!(edit(&|d| d["accessors"][0]["count"] = json!(1_000_000)).is_err());
        assert!(edit(&|d| d["accessors"][0]["count"] = json!(u64::MAX)).is_err());
        assert!(edit(&|d| d["accessors"][1]["count"] = json!(1_000_000)).is_err());
        assert!(edit(&|d| d["accessors"][1]["componentType"] = json!(5126)).is_err());
        assert!(edit(&|d| d["accessors"][0]["componentType"] = json!(1234)).is_err());
        assert!(edit(&|d| d["accessors"][0]["type"] = json!("MAT4")).is_err());
        assert!(edit(&|d| d["accessors"][0]["bufferView"] = json!(7)).is_err());
        assert!(edit(&|d| d["accessors"][0]["byteOffset"] = json!(u64::MAX)).is_err());
        assert!(edit(&|d| d["accessors"][0]["sparse"] = json!({})).is_err());
        assert!(edit(&|d| d["bufferViews"][0]["byteLength"] = json!(u64::MAX)).is_err());
        assert!(edit(&|d| d["bufferViews"][0]["byteStride"] = json!(4)).is_err());
        assert!(edit(&|d| d["bufferViews"][0]["byteStride"] = json!(100000)).is_err());
        assert!(edit(&|d| d["bufferViews"][0]["buffer"] = json!(3)).is_err());
        assert!(edit(&|d| d["buffers"][0]["uri"] = json!("x.bin")).is_err());
        assert!(edit(&|d| d["nodes"][0]["mesh"] = json!(9)).is_err()); // no such mesh: nothing to show
        assert!(edit(&|d| d["nodes"] = json!([{"children": [0], "mesh": 0}])).is_err()); // a cycle
        assert!(edit(&|d| d["nodes"][0]["matrix"] = json!([1, 2, 3])).is_err());
        assert!(edit(&|d| d["nodes"][0]["rotation"] = json!([0, 0, 0, 0])).is_err());
        assert!(edit(&|d| d["nodes"][0]["scale"] = json!([1e300, 1e300, 1e300])).is_err());
        assert!(edit(&|d| d["meshes"][0]["primitives"][0]["extensions"] = json!({"KHR_draco_mesh_compression": {}})).is_err());
        assert!(edit(&|d| d["extensionsRequired"] = json!(["KHR_draco_mesh_compression"])).is_err());
        assert!(edit(&|d| d["scenes"][0]["nodes"] = json!([5])).is_err());
        assert!(edit(&|d| d["meshes"][0]["primitives"][0]["mode"] = json!(1)).is_err()); // lines only: no shape
        // a lot of instances of a big mesh
        let wide: Vec<Value> = (0..2000).map(|_| json!({"mesh": 0})).collect();
        assert!(
            edit(&|d| {
                d["nodes"] = json!(wide);
                d["scenes"][0]["nodes"] = json!((0..2000).collect::<Vec<_>>());
            })
            .is_ok()
        );
        // broken containers
        assert!(read_glb(b"glTF").is_err());
        assert!(read_glb(b"glTF\x01\0\0\0\x0c\0\0\0").is_err());
        let g = glb(&good, &bin);
        for cut in 0..g.len() {
            let _ = read_glb(&g[..cut]);
        }
        let mut bad_len = g.clone();
        bad_len[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_glb(&bad_len).is_err());
        let mut bad_total = g.clone();
        bad_total[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_glb(&bad_total).is_err());
        let mut x = 5u32;
        for i in 0..g.len() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut c = g.clone();
            c[i] = (x >> 24) as u8;
            let _ = read_glb(&c);
        }
    }

    #[test]
    fn nesting_is_limited() {
        let bin = tri_bin();
        let n = 200;
        let nodes: Vec<Value> = (0..n).map(|i| if i + 1 < n { json!({"children": [i + 1], "mesh": 0}) } else { json!({"mesh": 0}) }).collect();
        assert!(read_glb(&glb(&tri_doc(json!(nodes), json!([0])), &bin)).is_err());
    }

    #[test]
    fn our_glb_writer_round_trips() {
        let s = sphere_mesh(7.0, 10, 20);
        let v = s.signed_volume();
        let g = export_formats::glb(&surface(&s), "s").unwrap();
        let m = read_glb(&g).unwrap();
        assert_eq!(m.triangles.len(), s.triangles.len());
        assert!((m.signed_volume() - v).abs() / v < 1e-5);
        let b = box_mesh([0.0; 3], [1.0, 2.0, 3.0]);
        let g = export_formats::glb(&surface(&b), "b").unwrap();
        assert!((read_glb(&g).unwrap().signed_volume() - 6.0).abs() < 1e-5);
    }
    #[test]
    fn glb_pbr_colour_and_unsupported_textures_are_explained() {
        let mut doc = tri_doc(json!([{"mesh":0}]), json!([0]));
        doc["materials"] = json!([{"pbrMetallicRoughness": {"baseColorFactor":[0.5,0.0,0.0,1.0],"roughnessFactor":0.25,"metallicFactor":0.75}}]);
        doc["meshes"][0]["primitives"][0]["material"] = json!(0);
        let g = glb(&doc, &tri_bin());
        let a = appearance_glb(&g);
        let m = a.material.unwrap();
        assert!(!a.varies);
        assert!((m.color[0] - 0.73536).abs() < 0.001);
        assert_eq!(m.color[1], 0.0);
        assert!((m.roughness - 0.25).abs() < 1e-6);
        assert!((m.metallic - 0.75).abs() < 1e-6);
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
        doc["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(0);
        let a = appearance_glb(&glb(&doc, &tri_bin()));
        assert!(a.varies);
        assert!(a.notes.iter().any(|n| n.contains("texture")));
        assert!(a.notes.iter().any(|n| n.contains("vertex colour")));
    }

    #[test]
    fn exported_glb_uniform_look_recovers_colour_and_roughness() {
        let s = surface(&box_mesh([0.0; 3], [1.0; 3]));
        let g = export_formats::glb(&s, "coloured").unwrap();
        let a = appearance_glb(&g);
        let m = a.material.unwrap();
        assert!(!a.varies);
        assert!((m.color[0] - 200.0 / 255.0).abs() < 0.002);
        assert!((m.color[1] - 100.0 / 255.0).abs() < 0.002);
        assert!((m.roughness - 100.0 / 255.0).abs() < 0.002);
    }
}
