//! Mesh import: our own readers for binary and ASCII STL, OBJ, PLY (ascii and binary) and, in
//! `import_3mf` and `import_glb`, 3MF and GLB. Every reader turns a file into an `ImportedMesh`
//! (millimetres) and `clean` checks it: finite numbers, indices in range, no degenerate triangles.
//!
//! Hostile files never panic and never allocate without bound: the file size (256 MB), the
//! triangle count (5 million) and the vertex count are all capped, and counts written in a file
//! are checked against the bytes that are really there before anything is reserved.
//!
//! The entry points are `import_file` / `import_bytes`; the helpers on `ImportedMesh` weld the
//! vertices, find the bounds, move the model to the middle of the floor and scale it.

use crate::export_mesh::FastMap;

/// A bigger file is refused.
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
/// More triangles than this are refused.
pub const MAX_TRIANGLES: usize = 5_000_000;
/// More vertices than this are refused (an STL of 5 million loose triangles has 15 million corners).
pub const MAX_VERTICES: usize = 15_000_000;
/// Coordinates beyond this (in file units) are taken for garbage.
pub const MAX_COORD: f32 = 1.0e9;

pub const TOO_BIG_FILE: &str = "File is too big (up to 256 MB)";
pub const TOO_MANY_TRIS: &str = "Too many triangles (up to 5 million)";
pub const TOO_MANY_VERTS: &str = "Too many vertices";
pub const CUT_OFF: &str = "File is cut off partway through";
pub const BAD_NUMBER: &str = "Bad number (NaN, infinite, or too large)";
pub const BAD_INDEX: &str = "Vertex index is out of range";
pub const NO_SHAPE: &str = "No shape found (0 triangles with area)";

/// A triangle mesh in millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedMesh {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Appearance information kept separate from geometry so the existing SDF baker is unchanged.
#[derive(Clone, Debug, Default)]
pub struct AppearanceInfo {
    pub material: Option<crate::model::Material>,
    pub varies: bool,
    pub notes: Vec<String>,
}

/// The file formats we can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshFormat {
    Stl,
    Obj,
    Ply,
    ThreeMf,
    Glb,
}

impl MeshFormat {
    pub fn label(self) -> &'static str {
        match self {
            MeshFormat::Stl => "STL",
            MeshFormat::Obj => "OBJ",
            MeshFormat::Ply => "PLY",
            MeshFormat::ThreeMf => "3MF",
            MeshFormat::Glb => "GLB",
        }
    }
    /// glTF and OBJ pictures are usually Y up; the other formats are Z up (as on export).
    pub fn y_up_by_default(self) -> bool {
        matches!(self, MeshFormat::Obj | MeshFormat::Glb)
    }
}

/// Options for `import_bytes` / `import_file`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImportOptions {
    /// `Some(true)`: the file is Y up and is turned to Z up; `Some(false)`: it already is Z up;
    /// `None`: decide by the format (`MeshFormat::y_up_by_default`).
    pub y_up: Option<bool>,
}

/// The result of an import.
#[derive(Clone, Debug)]
pub struct Imported {
    pub mesh: ImportedMesh,
    /// One material when the imported surface is uniform; otherwise a representative colour.
    pub appearance: Option<crate::model::Material>,
    pub format: MeshFormat,
    /// The mesh was turned from Y up to Z up.
    #[allow(dead_code)] // (the card turns the model itself, from the choice of 向き)
    pub turned_y_up: bool,
    /// Triangles thrown away because they have no area.
    pub dropped_triangles: usize,
    /// What `ImportedMesh::health` found (its words are already in `notes`).
    #[allow(dead_code)]
    pub health: Health,
    /// Short friendly remarks for the person (holes, flat meshes, ...).
    pub notes: Vec<String>,
}

/// How closed the surface is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Health {
    /// Edges that belong to one triangle only (holes in the surface).
    pub open_edges: usize,
    /// Edges shared by more than two triangles.
    pub nonmanifold_edges: usize,
    /// Edges shared by two triangles that run along it in the same direction (a reversed face).
    pub inconsistent_winding_edges: usize,
}

impl Health {
    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        self.open_edges == 0 && self.nonmanifold_edges == 0
    }
}

pub(crate) fn parse_f32(tok: &[u8]) -> Option<f32> {
    std::str::from_utf8(tok).ok()?.parse::<f32>().ok()
}

fn is_ws(b: &u8) -> bool {
    b.is_ascii_whitespace()
}

/// The whitespace-separated words of a line or a whole text.
fn words(s: &[u8]) -> impl Iterator<Item = &[u8]> {
    s.split(is_ws).filter(|w| !w.is_empty())
}

fn bits3(p: [f32; 3]) -> [u32; 3] {
    // + 0.0 turns -0.0 into 0.0
    [(p[0] + 0.0).to_bits(), (p[1] + 0.0).to_bits(), (p[2] + 0.0).to_bits()]
}

/// Collects points and merges the ones that are exactly equal (the corners of an STL).
#[derive(Default)]
struct Welder {
    map: FastMap<[u32; 3], u32>,
    positions: Vec<[f32; 3]>,
}

impl Welder {
    fn add(&mut self, p: [f32; 3]) -> Result<u32, String> {
        if !p.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
            return Err(BAD_NUMBER.into());
        }
        if let Some(i) = self.map.get(&bits3(p)) {
            return Ok(*i);
        }
        if self.positions.len() >= MAX_VERTICES {
            return Err(TOO_MANY_VERTS.into());
        }
        let i = self.positions.len() as u32;
        self.map.insert(bits3(p), i);
        self.positions.push(p);
        Ok(i)
    }
}

// ---------------------------------------------------------------- 3x4 transforms

/// An affine transform of points: `p' = M p + t`, rows of `[m0 m1 m2 t]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [[f64; 4]; 3]);

impl Affine {
    pub const IDENTITY: Affine = Affine([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }

    /// The transform that does `inner` first and then `self`.
    pub fn after(&self, inner: &Affine) -> Affine {
        let (a, b) = (&self.0, &inner.0);
        let mut r = [[0.0; 4]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let mut s = if j == 3 { a[i][3] } else { 0.0 };
                for k in 0..3 {
                    s += a[i][k] * b[k][j];
                }
                *cell = s;
            }
        }
        Affine(r)
    }

    pub fn determinant(&self) -> f64 {
        let m = &self.0;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    #[allow(dead_code)]
    pub fn is_finite(&self) -> bool {
        self.0.iter().flatten().all(|v| v.is_finite())
    }
}

/// Add a transformed copy of a mesh part to `out`. A mirroring transform flips the triangles so
/// that they keep facing outwards.
pub(crate) fn append_transformed(out: &mut ImportedMesh, positions: &[[f32; 3]], triangles: &[[u32; 3]], m: &Affine) -> Result<(), String> {
    if out.triangles.len() + triangles.len() > MAX_TRIANGLES {
        return Err(TOO_MANY_TRIS.into());
    }
    if out.positions.len() + positions.len() > MAX_VERTICES {
        return Err(TOO_MANY_VERTS.into());
    }
    let base = out.positions.len() as u32;
    let flip = m.determinant() < 0.0;
    for p in positions {
        let q = m.apply([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        let q = [q[0] as f32, q[1] as f32, q[2] as f32];
        if !q.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
            return Err(BAD_NUMBER.into());
        }
        out.positions.push(q);
    }
    let n = positions.len() as u32;
    for t in triangles {
        if t.iter().any(|i| *i >= n) {
            return Err(BAD_INDEX.into());
        }
        let (a, b, c) = (base + t[0], base + t[1], base + t[2]);
        out.triangles.push(if flip { [a, c, b] } else { [a, b, c] });
    }
    Ok(())
}

// ---------------------------------------------------------------- STL

fn read_stl_binary(b: &[u8]) -> Result<ImportedMesh, String> {
    let n = b.get(80..84).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize).ok_or(CUT_OFF)?;
    if n > MAX_TRIANGLES {
        return Err(TOO_MANY_TRIS.into());
    }
    let body = b.get(84..).ok_or(CUT_OFF)?;
    if body.len() / 50 < n {
        return Err(CUT_OFF.into());
    }
    let mut w = Welder::default();
    let mut triangles = Vec::with_capacity(n);
    for rec in body.as_chunks::<50>().0.iter().take(n) {
        let mut idx = [0u32; 3];
        for (k, slot) in idx.iter_mut().enumerate() {
            let o = 12 + 12 * k;
            let f = |a: usize| rec.get(o + a..o + a + 4).map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]])).unwrap_or(f32::NAN);
            *slot = w.add([f(0), f(4), f(8)])?;
        }
        triangles.push(idx);
    }
    Ok(ImportedMesh { positions: w.positions, triangles })
}

fn read_stl_ascii(b: &[u8]) -> Result<ImportedMesh, String> {
    let mut w = Welder::default();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut corner = [0u32; 3];
    let mut k = 0usize;
    let mut it = words(b);
    while let Some(tok) = it.next() {
        if !tok.eq_ignore_ascii_case(b"vertex") {
            continue;
        }
        let mut p = [0f32; 3];
        for c in &mut p {
            *c = it.next().and_then(parse_f32).ok_or(BAD_NUMBER)?;
        }
        if let Some(slot) = corner.get_mut(k) {
            *slot = w.add(p)?;
        }
        k += 1;
        if k == 3 {
            if triangles.len() >= MAX_TRIANGLES {
                return Err(TOO_MANY_TRIS.into());
            }
            triangles.push(corner);
            k = 0;
        }
    }
    if k != 0 {
        return Err(CUT_OFF.into());
    }
    Ok(ImportedMesh { positions: w.positions, triangles })
}

/// Binary STL if the size fits the triangle count (a binary header may start with "solid"),
/// otherwise ASCII when it says "solid".
fn read_stl(b: &[u8]) -> Result<ImportedMesh, String> {
    let exact = b.get(80..84).is_some_and(|x| 84 + 50 * (u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as u64) == b.len() as u64);
    let head = b.get(..b.len().min(512)).unwrap_or(b);
    let solid = head.iter().skip_while(|c| c.is_ascii_whitespace()).take(5).map(u8::to_ascii_lowercase).eq(b"solid".iter().copied());
    if exact {
        read_stl_binary(b)
    } else if solid {
        read_stl_ascii(b)
    } else {
        read_stl_binary(b)
    }
}

// ---------------------------------------------------------------- OBJ

/// The most corners one OBJ face may have.
const MAX_FACE_CORNERS: usize = 100_000;

fn read_obj(b: &[u8]) -> Result<ImportedMesh, String> {
    let mut m = ImportedMesh::default();
    for raw in b.split(|c| *c == b'\n') {
        // everything after a # is a comment, also at the end of a line
        let raw = raw.split(|c| *c == b'#').next().unwrap_or(&[]);
        let mut it = words(raw);
        match it.next() {
            Some(b"v") => {
                let mut p = [0f32; 3];
                for c in &mut p {
                    *c = it.next().and_then(parse_f32).ok_or(BAD_NUMBER)?;
                }
                if !p.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
                    return Err(BAD_NUMBER.into());
                }
                if m.positions.len() >= MAX_VERTICES {
                    return Err(TOO_MANY_VERTS.into());
                }
                m.positions.push(p);
            }
            Some(b"f") => {
                let n = m.positions.len() as i64;
                let mut idx: Vec<u32> = Vec::new();
                for tok in it {
                    let head = tok.split(|c| *c == b'/').next().unwrap_or(&[]);
                    let i: i64 = std::str::from_utf8(head).ok().and_then(|s| s.parse().ok()).ok_or(BAD_INDEX)?;
                    let at = if i > 0 {
                        i - 1
                    } else if i < 0 {
                        n + i
                    } else {
                        return Err(BAD_INDEX.into());
                    };
                    if at < 0 || at >= i64::from(u32::MAX) {
                        return Err(BAD_INDEX.into());
                    }
                    if idx.len() >= MAX_FACE_CORNERS {
                        return Err("A face has too many vertices".into());
                    }
                    idx.push(at as u32);
                }
                if idx.len() < 3 {
                    return Err("A face has fewer than 3 vertices".into());
                }
                for t in crate::polygon::triangulate(&m.positions, &idx) {
                    if m.triangles.len() >= MAX_TRIANGLES {
                        return Err(TOO_MANY_TRIS.into());
                    }
                    m.triangles.push(t);
                }
            }
            _ => {}
        }
    }
    Ok(m)
}

// ---------------------------------------------------------------- PLY

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Ty {
    fn parse(s: &str) -> Option<Ty> {
        Some(match s {
            "char" | "int8" => Ty::I8,
            "uchar" | "uint8" => Ty::U8,
            "short" | "int16" => Ty::I16,
            "ushort" | "uint16" => Ty::U16,
            "int" | "int32" => Ty::I32,
            "uint" | "uint32" => Ty::U32,
            "float" | "float32" => Ty::F32,
            "double" | "float64" => Ty::F64,
            _ => return None,
        })
    }
    fn size(self) -> usize {
        match self {
            Ty::I8 | Ty::U8 => 1,
            Ty::I16 | Ty::U16 => 2,
            Ty::I32 | Ty::U32 | Ty::F32 => 4,
            Ty::F64 => 8,
        }
    }
}

struct Prop {
    name: String,
    /// `Some(count type)` for a list.
    list: Option<Ty>,
    ty: Ty,
}

struct Element {
    name: String,
    count: usize,
    props: Vec<Prop>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Enc {
    Ascii,
    Little,
    Big,
}

/// Reads values one after another from the body of a PLY file.
struct Values<'a> {
    enc: Enc,
    body: &'a [u8],
    pos: usize,
}

impl Values<'_> {
    fn next(&mut self, ty: Ty) -> Result<f64, String> {
        if self.enc == Enc::Ascii {
            while self.body.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
                self.pos += 1;
            }
            let start = self.pos;
            while self.body.get(self.pos).is_some_and(|c| !c.is_ascii_whitespace()) {
                self.pos += 1;
            }
            let tok = self.body.get(start..self.pos).filter(|t| !t.is_empty()).ok_or(CUT_OFF)?;
            let s = std::str::from_utf8(tok).map_err(|_| BAD_NUMBER.to_string())?;
            return match ty {
                Ty::F32 | Ty::F64 => s.parse::<f64>().map_err(|_| BAD_NUMBER.to_string()),
                _ => s.parse::<i64>().map(|v| v as f64).or_else(|_| s.parse::<f64>()).map_err(|_| BAD_NUMBER.to_string()),
            };
        }
        let n = ty.size();
        let raw = self.body.get(self.pos..self.pos + n).ok_or(CUT_OFF)?;
        self.pos += n;
        let mut a = [0u8; 8];
        if let Some(dst) = a.get_mut(..n) {
            dst.copy_from_slice(raw);
        }
        if self.enc == Enc::Big
            && let Some(dst) = a.get_mut(..n)
        {
            dst.reverse();
        }
        Ok(match ty {
            Ty::I8 => f64::from(a[0] as i8),
            Ty::U8 => f64::from(a[0]),
            Ty::I16 => f64::from(i16::from_le_bytes([a[0], a[1]])),
            Ty::U16 => f64::from(u16::from_le_bytes([a[0], a[1]])),
            Ty::I32 => f64::from(i32::from_le_bytes([a[0], a[1], a[2], a[3]])),
            Ty::U32 => f64::from(u32::from_le_bytes([a[0], a[1], a[2], a[3]])),
            Ty::F32 => f64::from(f32::from_le_bytes([a[0], a[1], a[2], a[3]])),
            Ty::F64 => f64::from_le_bytes(a),
        })
    }
}

fn read_ply(b: &[u8]) -> Result<ImportedMesh, String> {
    const BAD: &str = "The PLY header is wrong";
    // Only an entire header line can terminate the header. A comment may
    // legally contain the text "end_header" without ending the header.
    let header_limit = b.len().min(1 << 16);
    let mut offset = 0usize;
    let mut boundary = None;
    for line in b[..header_limit].split_inclusive(|&byte| byte == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        let line_without_nl = &line[..line.len() - 1];
        let content = line_without_nl.strip_suffix(b"\r").unwrap_or(line_without_nl);
        if content == b"end_header" {
            boundary = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let (end, body_start) = boundary.ok_or(BAD)?;
    let head = std::str::from_utf8(b.get(..end).ok_or(BAD)?).map_err(|_| BAD.to_string())?;
    let mut enc = None;
    let mut elements: Vec<Element> = Vec::new();
    for (i, line) in head.lines().enumerate() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match (i, w.first().copied()) {
            (0, Some("ply")) => {}
            (0, _) => return Err(BAD.into()),
            (_, Some("format")) => {
                enc = Some(match w.get(1).copied() {
                    Some("ascii") => Enc::Ascii,
                    Some("binary_little_endian") => Enc::Little,
                    Some("binary_big_endian") => Enc::Big,
                    _ => return Err(BAD.into()),
                });
            }
            (_, Some("element")) => {
                let count: usize = w.get(2).and_then(|s| s.parse().ok()).ok_or(BAD)?;
                if elements.len() >= 32 {
                    return Err(BAD.into());
                }
                elements.push(Element { name: w.get(1).copied().unwrap_or("").to_string(), count, props: Vec::new() });
            }
            (_, Some("property")) => {
                let el = elements.last_mut().ok_or(BAD)?;
                if el.props.len() >= 64 {
                    return Err(BAD.into());
                }
                if w.get(1).copied() == Some("list") {
                    let ct = w.get(2).and_then(|s| Ty::parse(s)).ok_or(BAD)?;
                    let it = w.get(3).and_then(|s| Ty::parse(s)).ok_or(BAD)?;
                    el.props.push(Prop { name: w.get(4).copied().unwrap_or("").to_string(), list: Some(ct), ty: it });
                } else {
                    let ty = w.get(1).and_then(|s| Ty::parse(s)).ok_or(BAD)?;
                    el.props.push(Prop { name: w.get(2).copied().unwrap_or("").to_string(), list: None, ty });
                }
            }
            _ => {}
        }
    }
    let enc = enc.ok_or(BAD)?;
    let body = b.get(body_start..).ok_or(CUT_OFF)?;
    let mut v = Values { enc, body, pos: 0 };
    let mut m = ImportedMesh::default();
    let mut idx: Vec<u32> = Vec::new();
    // PLY elements may appear in any declared order, including faces before vertices.
    // Keep a compact offset table and triangulate only after all vertices are loaded.
    let mut face_ranges: Vec<(usize, usize)> = Vec::new();
    let mut face_indices: Vec<u32> = Vec::new();
    for el in &elements {
        // the body must be able to hold the rows before anything is reserved
        let remaining = body.len().saturating_sub(v.pos);
        // A property-less element consumes zero bytes but used to run its
        // declared count without any validation (even one billion iterations
        // for a roughly 60-byte PLY header). Such rows have no information.
        if el.count > 0 && el.props.is_empty() {
            return Err("A PLY element with rows must declare properties".into());
        }
        // Unknown elements need a limit too, not only vertices/faces.
        if el.count > MAX_VERTICES.max(MAX_TRIANGLES * 2) {
            return Err("Too many elements in the PLY".into());
        }
        let min_row: usize = if enc == Enc::Ascii { 1 } else { el.props.iter().map(|p| p.list.map_or(p.ty.size(), Ty::size)).sum() };
        if el.count.checked_mul(min_row.max(1)).is_none_or(|need| need > remaining) {
            return Err(CUT_OFF.into());
        }
        let is_vertex = el.name == "vertex";
        let is_face = el.name == "face";
        if is_vertex {
            if el.count > MAX_VERTICES {
                return Err(TOO_MANY_VERTS.into());
            }
            m.positions.reserve(el.count);
        }
        if is_face && el.count > MAX_TRIANGLES * 2 {
            return Err(TOO_MANY_TRIS.into());
        }
        let col = |name: &str| el.props.iter().position(|p| p.name == name && p.list.is_none());
        let (cx, cy, cz) = (col("x"), col("y"), col("z"));
        if is_vertex && (cx.is_none() || cy.is_none() || cz.is_none()) {
            return Err("The PLY has no x y z coordinates".into());
        }
        for _ in 0..el.count {
            let mut p = [0f64; 3];
            idx.clear();
            for (pi, prop) in el.props.iter().enumerate() {
                match prop.list {
                    None => {
                        let val = v.next(prop.ty)?;
                        if is_vertex {
                            if Some(pi) == cx {
                                p[0] = val;
                            } else if Some(pi) == cy {
                                p[1] = val;
                            } else if Some(pi) == cz {
                                p[2] = val;
                            }
                        }
                    }
                    Some(ct) => {
                        let k = v.next(ct)?;
                        if !(0.0..=1.0e6).contains(&k) {
                            return Err("The PLY face vertex count is wrong".into());
                        }
                        let keep = is_face && (prop.name == "vertex_indices" || prop.name == "vertex_index");
                        for _ in 0..k as usize {
                            let val = v.next(prop.ty)?;
                            if keep {
                                if !(0.0..4.0e9).contains(&val) {
                                    return Err(BAD_INDEX.into());
                                }
                                idx.push(val as u32);
                            }
                        }
                    }
                }
            }
            if is_vertex {
                let q = [p[0] as f32, p[1] as f32, p[2] as f32];
                if !q.iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
                    return Err(BAD_NUMBER.into());
                }
                m.positions.push(q);
            } else if is_face && idx.len() >= 3 {
                if face_indices.len().saturating_add(idx.len()) > MAX_TRIANGLES.saturating_mul(3) {
                    return Err(TOO_MANY_TRIS.into());
                }
                face_ranges.push((face_indices.len(), idx.len()));
                face_indices.extend_from_slice(&idx);
            }
        }
    }
    for (start, len) in face_ranges {
        let face = &face_indices[start..start + len];
        if face.iter().any(|&v| v as usize >= m.positions.len()) {
            return Err(BAD_INDEX.into());
        }
        for t in crate::polygon::triangulate(&m.positions, face) {
            if m.triangles.len() >= MAX_TRIANGLES {
                return Err(TOO_MANY_TRIS.into());
            }
            m.triangles.push(t);
        }
    }
    Ok(m)
}

// ---------------------------------------------------------------- checking and helpers

impl ImportedMesh {
    /// The corners of the box around the mesh, if it has points.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.positions.first()?;
        let (mut lo, mut hi) = (*first, *first);
        for p in &self.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        Some((lo, hi))
    }

    /// Width, depth and height of the box around the mesh.
    pub fn size(&self) -> [f32; 3] {
        self.bounds().map_or([0.0; 3], |(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]])
    }

    /// Merge points closer than `tol` (0 merges only identical points). Triangles that collapse
    /// are dropped and unused points are removed. Returns how many triangles were dropped.
    #[allow(dead_code)] // helpers of the readers' API, used by the tests
    pub fn weld(&mut self, tol: f32) -> usize {
        let mut remap = vec![0u32; self.positions.len()];
        let mut kept: Vec<[f32; 3]> = Vec::new();
        if tol > 0.0 && tol.is_finite() {
            let inv = 1.0 / f64::from(tol);
            let mut map: FastMap<[i64; 3], u32> = FastMap::default();
            for (i, p) in self.positions.iter().enumerate() {
                let key = [(f64::from(p[0]) * inv).round() as i64, (f64::from(p[1]) * inv).round() as i64, (f64::from(p[2]) * inv).round() as i64];
                let id = *map.entry(key).or_insert_with(|| {
                    kept.push(*p);
                    (kept.len() - 1) as u32
                });
                if let Some(r) = remap.get_mut(i) {
                    *r = id;
                }
            }
        } else {
            let mut map: FastMap<[u32; 3], u32> = FastMap::default();
            for (i, p) in self.positions.iter().enumerate() {
                let id = *map.entry(bits3(*p)).or_insert_with(|| {
                    kept.push(*p);
                    (kept.len() - 1) as u32
                });
                if let Some(r) = remap.get_mut(i) {
                    *r = id;
                }
            }
        }
        let before = self.triangles.len();
        let mut tris = Vec::with_capacity(before);
        for t in &self.triangles {
            let (Some(&a), Some(&b), Some(&c)) = (remap.get(t[0] as usize), remap.get(t[1] as usize), remap.get(t[2] as usize)) else { continue };
            if a != b && b != c && a != c {
                tris.push([a, b, c]);
            }
        }
        self.positions = kept;
        self.triangles = tris;
        self.compact();
        before - self.triangles.len()
    }

    /// Remove the points no triangle uses.
    fn compact(&mut self) {
        let mut used = vec![false; self.positions.len()];
        for t in &self.triangles {
            for i in t {
                if let Some(u) = used.get_mut(*i as usize) {
                    *u = true;
                }
            }
        }
        if used.iter().all(|u| *u) {
            return;
        }
        let mut remap = vec![0u32; self.positions.len()];
        let mut kept = Vec::with_capacity(self.positions.len());
        for (i, p) in self.positions.iter().enumerate() {
            if used.get(i).copied().unwrap_or(false) {
                if let Some(r) = remap.get_mut(i) {
                    *r = kept.len() as u32;
                }
                kept.push(*p);
            }
        }
        for t in &mut self.triangles {
            for i in t.iter_mut() {
                *i = remap.get(*i as usize).copied().unwrap_or(0);
            }
        }
        self.positions = kept;
    }

    /// Turn a Y-up model to Z up: `(x, y, z)` becomes `(x, -z, y)`. This undoes the Y-up export.
    pub fn y_up_to_z_up(&mut self) {
        for p in &mut self.positions {
            *p = [p[0], -p[2], p[1]];
        }
    }

    /// Move the model so that its bottom is at z = 0 and its middle is at x = y = 0.
    #[allow(dead_code)] // helpers of the readers' API, used by the tests
    pub fn centre_and_ground(&mut self) {
        let Some((lo, hi)) = self.bounds() else { return };
        let (cx, cy) = ((lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5);
        for p in &mut self.positions {
            *p = [p[0] - cx, p[1] - cy, p[2] - lo[2]];
        }
    }

    /// Scale about the origin.
    pub fn scale(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        for p in &mut self.positions {
            *p = [p[0] * factor, p[1] * factor, p[2] * factor];
        }
    }

    /// Scale about the origin so that the longest side of the box is `target_mm`.
    #[allow(dead_code)] // helpers of the readers' API, used by the tests
    pub fn scale_to_size(&mut self, target_mm: f32) {
        let longest = self.size().into_iter().fold(0.0f32, f32::max);
        if longest > 0.0 && target_mm.is_finite() && target_mm > 0.0 {
            self.scale(((f64::from(target_mm)) / f64::from(longest)) as f32);
        }
    }

    /// The volume inside the surface (positive when the triangles face outwards).
    #[allow(dead_code)] // helpers of the readers' API, used by the tests
    pub fn signed_volume(&self) -> f64 {
        let at = |i: u32| self.positions.get(i as usize).map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        let mut v = 0.0;
        for t in &self.triangles {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
            }
        }
        v / 6.0
    }

    /// The area of the surface.
    #[allow(dead_code)] // helpers of the readers' API, used by the tests
    pub fn surface_area(&self) -> f64 {
        let at = |i: u32| self.positions.get(i as usize).map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        let mut s = 0.0;
        for t in &self.triangles {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let x = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
                s += 0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            }
        }
        s
    }

    /// Count the edges that are open or shared by too many triangles (the points should be welded).
    pub fn health(&self) -> Health {
        // (edge, direction): the direction is the lowest bit, so equal edges sit together
        let mut edges: Vec<u128> = Vec::with_capacity(self.triangles.len() * 3);
        for t in &self.triangles {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                let key = (u64::from(a.min(b)) << 32) | u64::from(a.max(b));
                edges.push((u128::from(key) << 1) | u128::from(a > b));
            }
        }
        edges.sort_unstable();
        let mut h = Health::default();
        let mut i = 0;
        while i < edges.len() {
            let key = edges.get(i).map(|e| e >> 1);
            let mut j = i;
            while j < edges.len() && edges.get(j).map(|e| e >> 1) == key {
                j += 1;
            }
            match j - i {
                1 => h.open_edges += 1,
                2 => {
                    if edges.get(i).map(|e| e & 1) == edges.get(i + 1).map(|e| e & 1) {
                        h.inconsistent_winding_edges += 1;
                    }
                }
                _ => h.nonmanifold_edges += 1,
            }
            i = j;
        }
        h
    }
}

/// Check a freshly read mesh: finite numbers, indices in range, no triangle without area.
/// Unused points are removed. Returns the clean mesh and the number of triangles dropped.
pub fn clean(mut m: ImportedMesh) -> Result<(ImportedMesh, usize), String> {
    if m.triangles.len() > MAX_TRIANGLES {
        return Err(TOO_MANY_TRIS.into());
    }
    if m.positions.len() > MAX_VERTICES {
        return Err(TOO_MANY_VERTS.into());
    }
    if !m.positions.iter().flatten().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
        return Err(BAD_NUMBER.into());
    }
    let n = m.positions.len();
    if m.triangles.iter().flatten().any(|i| *i as usize >= n) {
        return Err(BAD_INDEX.into());
    }
    let before = m.triangles.len();
    let pos = std::mem::take(&mut m.positions);
    m.triangles.retain(|t| {
        let at = |i: u32| pos.get(i as usize).map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) else { return false };
        let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let x = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        x[0] * x[0] + x[1] * x[1] + x[2] * x[2] > 0.0
    });
    m.positions = pos;
    if m.triangles.is_empty() {
        return Err(NO_SHAPE.into());
    }
    m.compact();
    let dropped = before - m.triangles.len();
    Ok((m, dropped))
}

// ---------------------------------------------------------------- entry points

fn lower_ext(name: &str) -> String {
    std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Which format a file is, by its first bytes and then by its name.
pub fn sniff(b: &[u8], name: &str) -> Option<MeshFormat> {
    if b.starts_with(b"PK\x03\x04") || b.starts_with(b"PK\x05\x06") {
        return Some(MeshFormat::ThreeMf);
    }
    if b.starts_with(b"glTF") {
        return Some(MeshFormat::Glb);
    }
    if b.starts_with(b"ply\n") || b.starts_with(b"ply\r") {
        return Some(MeshFormat::Ply);
    }
    match lower_ext(name).as_str() {
        "stl" => return Some(MeshFormat::Stl),
        "obj" => return Some(MeshFormat::Obj),
        "ply" => return Some(MeshFormat::Ply),
        "3mf" => return Some(MeshFormat::ThreeMf),
        "glb" => return Some(MeshFormat::Glb),
        _ => {}
    }
    // no usable name: look at the text
    let head = b.get(..b.len().min(4096)).unwrap_or(b);
    let text = String::from_utf8_lossy(head);
    let first = text.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if first == "solid" {
        return Some(MeshFormat::Stl);
    }
    if text.lines().any(|l| l.starts_with("v ") || l.starts_with("f ")) {
        return Some(MeshFormat::Obj);
    }
    None
}

/// OBJ colours live in a separate .mtl file that is not read: say so rather than lose them silently.
fn appearance_obj(bytes: &[u8]) -> AppearanceInfo {
    let mut out = AppearanceInfo::default();
    let names_materials = bytes.split(|c| *c == b'\n').any(|line| {
        let mut it = words(line);
        matches!(it.next(), Some(b"mtllib" | b"usemtl"))
    });
    if names_materials {
        out.notes.push("The OBJ's material file (.mtl) is not read, so its colours and materials are not kept".into());
    }
    // `v x y z r g b`: vertex colours. A shape has one colour, so it takes their average and says so (#190)
    let (mut sum, mut n, mut first, mut varies) = ([0f64; 3], 0u64, None::<[f32; 3]>, false);
    for line in bytes.split(|c| *c == b'\n') {
        let line = line.split(|c| *c == b'#').next().unwrap_or(&[]);
        let mut it = words(line);
        if it.next() != Some(b"v") {
            continue;
        }
        let nums: Vec<f32> = it.filter_map(parse_f32).take(7).collect();
        if nums.len() < 6 {
            continue;
        }
        let mut c = [nums[3], nums[4], nums[5]];
        // some writers use 0..255
        if c.iter().any(|v| *v > 1.0) {
            c = c.map(|v| v / 255.0);
        }
        if !c.iter().all(|v| v.is_finite()) {
            continue;
        }
        let c = c.map(|v| v.clamp(0.0, 1.0));
        match first {
            None => first = Some(c),
            Some(f) if (0..3).any(|i| (f[i] - c[i]).abs() > 1.0 / 255.0) => varies = true,
            _ => {}
        }
        for i in 0..3 {
            sum[i] += f64::from(c[i]);
        }
        n += 1;
    }
    if n > 0 {
        let avg = sum.map(|v| (v / n as f64) as f32);
        out.material = Some(crate::model::Material::plain(avg, 0.5, 0.0));
        // several colours: the importer's usual note says one main colour is used
        out.varies = varies;
    }
    out
}

/// PLY vertex colours are not kept (a shape has one colour): say so rather than lose them silently.
fn appearance_ply(bytes: &[u8]) -> AppearanceInfo {
    let mut out = AppearanceInfo::default();
    let head = bytes.get(..bytes.len().min(16 * 1024)).unwrap_or(bytes);
    let end = head.windows(10).position(|w| w == b"end_header").unwrap_or(head.len());
    let header = String::from_utf8_lossy(head.get(..end).unwrap_or(head)).to_ascii_lowercase();
    let coloured = header.lines().any(|l| {
        let mut w = l.split_whitespace();
        w.next() == Some("property") && matches!(w.last(), Some("red" | "green" | "blue" | "diffuse_red" | "diffuse_green" | "diffuse_blue"))
    });
    if coloured {
        out.notes.push("The PLY's vertex colours are not kept; the shape gets one colour".into());
    }
    out
}

/// Read a mesh from the bytes of a file. `name` is only used to tell the format.
pub fn import_bytes(bytes: &[u8], name: &str, opts: &ImportOptions) -> Result<Imported, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(TOO_BIG_FILE.into());
    }
    let format = sniff(bytes, name).ok_or("Can't tell this file's format (supports STL, OBJ, PLY, 3MF, GLB)")?;
    let appearance_info = match format {
        MeshFormat::ThreeMf => crate::import_3mf::appearance_3mf(bytes),
        MeshFormat::Glb => crate::import_glb::appearance_glb(bytes),
        MeshFormat::Obj => appearance_obj(bytes),
        MeshFormat::Ply => appearance_ply(bytes),
        MeshFormat::Stl => Default::default(),
    };
    let raw = match format {
        MeshFormat::Stl => read_stl(bytes),
        MeshFormat::Obj => read_obj(bytes),
        MeshFormat::Ply => read_ply(bytes),
        MeshFormat::ThreeMf => crate::import_3mf::read_3mf(bytes),
        MeshFormat::Glb => crate::import_glb::read_glb(bytes),
    }?;
    let (mut mesh, dropped_triangles) = clean(raw)?;
    let y_up = opts.y_up.unwrap_or_else(|| format.y_up_by_default());
    if y_up {
        mesh.y_up_to_z_up();
    }
    let health = mesh.health();
    let mut notes = appearance_info.notes;
    if appearance_info.varies {
        notes.push("Several colours or materials found. SDF can't keep colours per face, so one main colour is used (the original colour split is not saved)".into());
    }
    if health.open_edges > 0 {
        notes.push(format!("There are holes (open edges: {}). The shape is loaded approximately", health.open_edges));
    }
    if health.nonmanifold_edges > 0 {
        notes.push(format!("Some edges have 3 or more faces meeting ({} edges)", health.nonmanifold_edges));
    }
    if health.inconsistent_winding_edges > 0 {
        notes.push(format!("Some faces are turned the wrong way ({} edges). Inside and outside may be mixed up", health.inconsistent_winding_edges));
    }
    if dropped_triangles > 0 {
        notes.push(format!("Removed {dropped_triangles} zero-area triangles"));
    }
    let s = mesh.size();
    let longest = s.into_iter().fold(0.0f32, f32::max);
    if longest > 0.0 && s.iter().any(|d| *d < longest * 1e-6) {
        notes.push("Flat shape (no thickness)".to_string());
    }
    Ok(Imported { mesh, appearance: appearance_info.material, format, turned_y_up: y_up, dropped_triangles, health, notes })
}

pub const IMPORT_CANCELLED: &str = "Import cancelled";

/// Read a mesh file (refusing anything over 256 MB).
#[cfg_attr(not(test), allow(dead_code))]
pub fn import_file(path: &std::path::Path, opts: &ImportOptions) -> Result<Imported, String> {
    import_file_with_control(path, opts, &std::sync::atomic::AtomicBool::new(false), &std::sync::atomic::AtomicU32::new(0))
}

/// The file read checks cancellation every 256 KiB, not only after allocating the entire
/// input. Parsing/cleaning is not interruptible internally yet; its result is discarded if
/// cancellation arrived during that step, so no partial shape can be committed.
pub fn import_file_with_control(
    path: &std::path::Path,
    opts: &ImportOptions,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &std::sync::atomic::AtomicU32,
) -> Result<Imported, String> {
    use std::io::Read;
    use std::sync::atomic::Ordering;
    let cancelled = || cancel.load(Ordering::Relaxed);
    if cancelled() { return Err(IMPORT_CANCELLED.into()); }
    let mut f = std::fs::File::open(path).map_err(|e| format!("Can't open the file: {e}"))?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len > MAX_FILE_BYTES as u64 {
        return Err(TOO_BIG_FILE.into());
    }
    let mut bytes = Vec::with_capacity(len.min(MAX_FILE_BYTES as u64) as usize);
    let mut buf = [0u8; 256 * 1024];
    loop {
        if cancelled() { return Err(IMPORT_CANCELLED.into()); }
        let n = f.read(&mut buf).map_err(|e| format!("Can't read the file: {e}"))?;
        if n == 0 { break; }
        if bytes.len().saturating_add(n) > MAX_FILE_BYTES {
            return Err(TOO_BIG_FILE.into());
        }
        bytes.extend_from_slice(&buf[..n]);
        if len > 0 {
            progress.store(((bytes.len() as f64 / len as f64).min(1.0) * 500.0) as u32, Ordering::Relaxed);
        }
    }
    progress.store(500, Ordering::Relaxed);
    if cancelled() { return Err(IMPORT_CANCELLED.into()); }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let loaded = import_bytes(&bytes, &name, opts)?;
    if cancelled() { return Err(IMPORT_CANCELLED.into()); }
    progress.store(1000, Ordering::Relaxed);
    Ok(loaded)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn obj_vertex_colours_give_the_shape_their_colour() {
        let obj = b"v 0 0 0 1 0 0\nv 10 0 0 1 0 0\nv 0 10 0 1 0 0\nv 0 0 10 1 0 0\nf 1 3 2\nf 1 2 4\nf 2 3 4\nf 3 1 4\n";
        let got = import_bytes(obj, "red.obj", &ImportOptions { y_up: Some(false) }).expect("reads");
        let m = got.appearance.expect("the vertex colour is kept (#190)");
        assert!((m.color[0] - 1.0).abs() < 1e-3 && m.color[1] < 1e-3 && m.color[2] < 1e-3, "{:?}", m.color);
        let two = b"v 0 0 0 255 0 0\nv 10 0 0 0 0 255\nv 0 10 0 0 0 255\nv 0 0 10 255 0 0\nf 1 3 2\nf 1 2 4\nf 2 3 4\nf 3 1 4\n";
        let got = import_bytes(two, "two.obj", &ImportOptions { y_up: Some(false) }).expect("reads");
        assert!(got.appearance.is_some());
        assert!(got.notes.iter().any(|n| n.contains("Several colours")), "{:?}", got.notes);
    }
    use crate::export_formats::{self, Look, Mat8, Surface, Units};
    use solidcraft_geom::melt::TriMesh;

    #[test]
    fn pre_cancelled_mesh_read_does_not_parse_or_allocate_a_model() {
        use std::sync::atomic::{AtomicBool, AtomicU32};
        let cancelled = AtomicBool::new(true);
        let progress = AtomicU32::new(0);
        let bad_path = std::path::Path::new("file-that-does-not-need-to-exist.obj");
        assert_eq!(import_file_with_control(bad_path, &ImportOptions::default(), &cancelled, &progress).unwrap_err(), IMPORT_CANCELLED);
        assert_eq!(progress.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    /// A closed unit-ish box from `lo` to `hi`, outward facing, 12 triangles.
    pub fn box_mesh(lo: [f32; 3], hi: [f32; 3]) -> ImportedMesh {
        let p = |x: usize, y: usize, z: usize| {
            [if x == 0 { lo[0] } else { hi[0] }, if y == 0 { lo[1] } else { hi[1] }, if z == 0 { lo[2] } else { hi[2] }]
        };
        let mut positions = Vec::new();
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    positions.push(p(x, y, z));
                }
            }
        }
        // index = x + 2y + 4z
        let triangles = vec![
            [0, 2, 1],
            [1, 2, 3], // bottom (-z)
            [4, 5, 6],
            [5, 7, 6], // top (+z)
            [0, 1, 4],
            [1, 5, 4], // front (-y)
            [2, 6, 3],
            [3, 6, 7], // back (+y)
            [0, 4, 2],
            [2, 4, 6], // left (-x)
            [1, 3, 5],
            [3, 7, 5], // right (+x)
        ];
        ImportedMesh { positions, triangles }
    }

    /// A UV sphere with outward triangles.
    pub fn sphere_mesh(r: f32, rings: usize, segs: usize) -> ImportedMesh {
        let mut positions = vec![[0.0, 0.0, r]];
        for i in 1..rings {
            let th = std::f32::consts::PI * i as f32 / rings as f32;
            for j in 0..segs {
                let ph = std::f32::consts::TAU * j as f32 / segs as f32;
                positions.push([r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()]);
            }
        }
        positions.push([0.0, 0.0, -r]);
        let last = (positions.len() - 1) as u32;
        let ring = |i: usize, j: usize| (1 + (i - 1) * segs + j % segs) as u32;
        let mut triangles = Vec::new();
        for j in 0..segs {
            triangles.push([0, ring(1, j), ring(1, j + 1)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                let (a, b, c, d) = (ring(i, j), ring(i, j + 1), ring(i + 1, j), ring(i + 1, j + 1));
                triangles.push([a, c, b]);
                triangles.push([b, c, d]);
            }
        }
        for j in 0..segs {
            triangles.push([last, ring(rings - 1, j + 1), ring(rings - 1, j)]);
        }
        ImportedMesh { positions, triangles }
    }

    pub fn to_trimesh(m: &ImportedMesh) -> TriMesh {
        TriMesh {
            positions: m.positions.iter().map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]).collect(),
            triangles: m.triangles.clone(),
        }
    }

    pub fn surface(m: &ImportedMesh) -> Surface {
        Surface { mesh: to_trimesh(m), look: Look::uniform(Mat8 { rgb: [200, 100, 50], rough: 100, metal: 0 }) }
    }

    fn opts() -> ImportOptions {
        ImportOptions { y_up: Some(false) }
    }

    fn load(bytes: &[u8], name: &str) -> Imported {
        import_bytes(bytes, name, &opts()).unwrap()
    }

    #[test]
    fn helpers_make_closed_meshes_with_the_right_volume() {
        let b = box_mesh([0.0; 3], [2.0, 3.0, 4.0]);
        assert!((b.signed_volume() - 24.0).abs() < 1e-9);
        assert!((b.surface_area() - 2.0 * (6.0 + 8.0 + 12.0)).abs() < 1e-9);
        assert!(b.health().is_closed());
        let s = sphere_mesh(10.0, 24, 48);
        let v = 4.0 / 3.0 * std::f64::consts::PI * 1000.0;
        assert!((s.signed_volume() - v).abs() / v < 0.01, "{}", s.signed_volume());
        assert!(s.health().is_closed());
    }

    const ASCII_TETRA: &str = "solid t\n facet normal 0 0 -1\n  outer loop\n   vertex 0 0 0\n   vertex 0 1 0\n   vertex 1 0 0\n  endloop\n endfacet\n\
        facet normal 0 -1 0\n outer loop\n vertex 0 0 0\n vertex 1 0 0\n vertex 0 0 1\n endloop\n endfacet\n\
        facet normal -1 0 0\n outer loop\n vertex 0 0 0\n vertex 0 0 1\n vertex 0 1 0\n endloop\n endfacet\n\
        facet normal 1 1 1\n outer loop\n vertex 1 0 0\n vertex 0 1 0\n vertex 0 0 1\n endloop\n endfacet\nendsolid t\n";

    #[test]
    fn propertyless_ply_element_cannot_force_a_billion_empty_iterations() {
        let malicious = b"ply\nformat ascii 1.0\nelement dummy 1000000000\nend_header\n";
        assert!(read_ply(malicious).is_err());
        let binary = b"ply\nformat binary_little_endian 1.0\nelement dummy 1000000000\nend_header\n";
        assert!(read_ply(binary).is_err());
        // A zero-row dummy element is harmless and should still be accepted
        // alongside valid geometry by the normal reader.
        let valid = b"ply\nformat ascii 1.0\nelement dummy 0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n";
        assert_eq!(read_ply(valid).unwrap().triangles.len(), 1);
    }

    #[test]
    fn ascii_stl_by_hand() {
        let r = load(ASCII_TETRA.as_bytes(), "x.stl");
        assert_eq!((r.mesh.positions.len(), r.mesh.triangles.len()), (4, 4));
        assert!((r.mesh.signed_volume() - 1.0 / 6.0).abs() < 1e-9);
        assert!(r.health.is_closed());
        // upper case, exponent forms, windows line ends, no extension
        let t = ASCII_TETRA.to_uppercase().replace('\n', "\r\n").replace("1 0 0", "1e0 0 0.0E0");
        assert_eq!(load(t.as_bytes(), "").mesh.triangles.len(), 4);
        // two solids in one file
        let two = format!("{ASCII_TETRA}{ASCII_TETRA}");
        assert_eq!(load(two.as_bytes(), "a.stl").mesh.triangles.len(), 8);
    }

    #[test]
    fn binary_stl_that_starts_with_solid_is_still_binary() {
        let b = box_mesh([0.0; 3], [1.0; 3]);
        let mut bin = export_formats::stl_binary(&to_trimesh(&b), "box");
        assert_eq!(&bin[..5], b"MELTC");
        bin[..5].copy_from_slice(b"solid");
        let r = load(&bin, "box.stl");
        assert_eq!(r.mesh.triangles.len(), 12);
        assert_eq!(r.mesh.positions.len(), 8);
        assert!((r.mesh.signed_volume() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hostile_stl_files() {
        // the count says a million triangles, the file has 3
        let mut b = vec![0u8; 84 + 150];
        b[80..84].copy_from_slice(&1_000_000u32.to_le_bytes());
        assert_eq!(import_bytes(&b, "a.stl", &opts()).unwrap_err(), CUT_OFF);
        // the count is above the cap
        b[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(import_bytes(&b, "a.stl", &opts()).unwrap_err(), TOO_MANY_TRIS);
        // NaN and infinity
        let mut one = vec![0u8; 84 + 50];
        one[80..84].copy_from_slice(&1u32.to_le_bytes());
        for (k, v) in [f32::NAN, f32::INFINITY, 1.0e30].into_iter().enumerate() {
            let mut c = one.clone();
            c[96 + 4 * k..100 + 4 * k].copy_from_slice(&v.to_le_bytes());
            assert!(import_bytes(&c, "a.stl", &opts()).is_err());
        }
        // all zero: a triangle without area
        assert_eq!(import_bytes(&one, "a.stl", &opts()).unwrap_err(), NO_SHAPE);
        // truncated and tiny files
        for n in [0usize, 1, 5, 83, 84, 100] {
            assert!(import_bytes(&vec![7u8; n], "a.stl", &opts()).is_err());
        }
        // ascii: a vertex short, a bad number, nan
        assert!(import_bytes(b"solid x\nvertex 0 0 0\nvertex 1 0 0\n", "a.stl", &opts()).is_err());
        assert!(import_bytes(b"solid x\nvertex 0 0 0\nvertex 1 0 abc\nvertex 0 1 0\n", "a.stl", &opts()).is_err());
        assert!(import_bytes(b"solid x\nvertex 0 0 0\nvertex 1 0 nan\nvertex 0 1 0\n", "a.stl", &opts()).is_err());
        assert!(import_bytes(b"solid x\nvertex 0 0 0\nvertex 1 0 inf\nvertex 0 1 0\n", "a.stl", &opts()).is_err());
        assert!(import_bytes(b"solid empty\nendsolid empty\n", "a.stl", &opts()).is_err());
    }

    #[test]
    fn obj_by_hand_quads_ngons_negative_indices_and_slashes() {
        let obj = "# cube\nmtllib x.mtl\no cube\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1 1.0\nv 1 0 1 0.5 0.5 0.5\nv 1 1 1\nv 0 1 1\n\
            vt 0 0\nvn 0 0 1\ng a\nusemtl m\ns off\n\
            f 1//1 4//1 3//1 2//1\nf 5/1/1 6/1/1 7/1/1 8/1/1\nf 1/1 2/1 6/1 5/1\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let r = load(obj.as_bytes(), "c.obj");
        assert_eq!(r.mesh.triangles.len(), 12);
        assert!((r.mesh.signed_volume() - 1.0).abs() < 1e-6, "{}", r.mesh.signed_volume());
        assert!(r.health.is_closed());
        // negative indices count from the last vertex so far; a pentagon fans into 3 triangles
        let neg = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0.5 1.5 0\nv 0 1 0\nf -5 -4 -3 -2 -1\n";
        let r = load(neg.as_bytes(), "p.obj");
        assert_eq!(r.mesh.triangles.len(), 3);
        assert!((r.mesh.surface_area() - 1.25).abs() < 1e-6);
        // windows line ends and a missing final newline
        assert_eq!(load("v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 2 3".as_bytes(), "t.obj").mesh.triangles.len(), 1);
    }

    #[test]
    fn obj_comments_may_end_a_line_and_a_concave_face_keeps_its_hole() {
        let r = load("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3 # triangle\n".as_bytes(), "c.obj");
        assert_eq!(r.mesh.triangles.len(), 1);
        // the U of the report: the notch is outside the face (area 4*4 - 2*3 = 10)
        let u = "v 0 0 0\nv 4 0 0\nv 4 4 0\nv 3 4 0\nv 3 1 0\nv 1 1 0\nv 1 4 0\nv 0 4 0\nf 1 2 3 4 5 6 7 8\n";
        let r = load(u.as_bytes(), "u.obj");
        assert_eq!(r.mesh.triangles.len(), 6);
        assert!((r.mesh.surface_area() - 10.0).abs() < 1e-4, "{}", r.mesh.surface_area());
    }

    #[test]
    fn ply_header_end_marker_inside_comment_is_not_a_terminator() {
        let normal = ply_ascii();
        let with_comment = normal.replace("comment tetra", "comment end_header is mentioned here");
        let base = load(normal.as_bytes(), "tetra.ply");
        let again = load(with_comment.as_bytes(), "tetra.ply");
        assert_eq!(base.mesh.positions, again.mesh.positions);
        assert_eq!(base.mesh.triangles, again.mesh.triangles);
    }

    #[test]
    fn ply_faces_before_vertices_still_triangulate_concave_polygons() {
        let ply = "ply\nformat ascii 1.0\nelement face 1\nproperty list uchar int vertex_indices\nelement vertex 8\nproperty float x\nproperty float y\nproperty float z\nend_header\n\
            8 0 1 2 3 4 5 6 7\n0 0 0\n4 0 0\n4 4 0\n3 4 0\n3 1 0\n1 1 0\n1 4 0\n0 4 0\n";
        let imported = load(ply.as_bytes(), "concave.ply");
        assert!((imported.mesh.surface_area() - 10.0).abs() < 1e-4, "{}", imported.mesh.surface_area());
        assert_eq!(imported.mesh.triangles.len(), 6);
    }

    #[test]
    fn a_concave_ply_face_is_cut_inside_its_outline_too() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 8\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n\
            0 0 0\n4 0 0\n4 4 0\n3 4 0\n3 1 0\n1 1 0\n1 4 0\n0 4 0\n8 0 1 2 3 4 5 6 7\n";
        let r = load(ply.as_bytes(), "u.ply");
        assert!((r.mesh.surface_area() - 10.0).abs() < 1e-4, "{}", r.mesh.surface_area());
    }

    #[test]
    fn colours_that_cannot_be_kept_are_reported() {
        let obj = "mtllib a.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl red\nf 1 2 3\n";
        let r = import_bytes(obj.as_bytes(), "a.obj", &opts()).unwrap();
        assert!(r.notes.iter().any(|n| n.contains(".mtl")), "{:?}", r.notes);
        let plain = import_bytes(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", "a.obj", &opts()).unwrap();
        assert!(plain.notes.iter().all(|n| !n.contains(".mtl")));
        let ply = "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 255 0 0\n1 0 0 255 0 0\n0 1 0 255 0 0\n3 0 1 2\n";
        let r = import_bytes(ply.as_bytes(), "a.ply", &opts()).unwrap();
        assert!(r.notes.iter().any(|n| n.contains("vertex colours")), "{:?}", r.notes);
    }

    #[test]
    fn hostile_obj_files() {
        for bad in [
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -4 1 2\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 x\n",
            "v 0 0 0\nv 1 0\nv 0 1 0\nf 1 2 3\n",
            "v 0 0 0\nv 1 0 nan\nv 0 1 0\nf 1 2 3\n",
            "v 0 0 0\nv 1 0 1e39\nv 0 1 0\nf 1 2 3\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 99999999999999999999 1 2\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 1 1\n",
            "",
            "f 1 2 3\n",
        ] {
            assert!(import_bytes(bad.as_bytes(), "x.obj", &opts()).is_err(), "{bad:?}");
        }
        // forward references are fine when the vertices come later; here they never do
        assert!(import_bytes(b"f 1 2 3\nv 0 0 0\nv 1 0 0\nv 0 1 0\n", "x.obj", &opts()).is_ok());
        // binary junk with an .obj name
        let junk: Vec<u8> = (0..5000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        assert!(import_bytes(&junk, "x.obj", &opts()).is_err());
    }

    fn ply_ascii() -> String {
        "ply\nformat ascii 1.0\ncomment tetra\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\n\
         element face 4\nproperty list uchar int vertex_indices\nelement edge 1\nproperty int vertex1\nproperty int vertex2\nend_header\n\
         0 0 0 255\n1 0 0 255\n0 1 0 255\n0 0 1 255\n3 0 2 1\n3 0 1 3\n3 0 3 2\n3 1 2 3\n0 1\n"
            .to_string()
    }

    #[test]
    fn ply_ascii_binary_and_big_endian_by_hand() {
        let r = load(ply_ascii().as_bytes(), "t.ply");
        assert_eq!((r.mesh.positions.len(), r.mesh.triangles.len()), (4, 4));
        assert!((r.mesh.signed_volume() - 1.0 / 6.0).abs() < 1e-9);
        // binary little endian with a quad, double coordinates and a short index list
        let mut b = b"ply\nformat binary_little_endian 1.0\nelement vertex 4\nproperty double x\nproperty double y\nproperty double z\nelement face 1\nproperty list uchar ushort vertex_index\nend_header\n".to_vec();
        for p in [[0.0f64, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 0.0]] {
            for c in p {
                b.extend_from_slice(&c.to_le_bytes());
            }
        }
        b.push(4);
        for i in 0u16..4 {
            b.extend_from_slice(&i.to_le_bytes());
        }
        let r = load(&b, "q.ply");
        assert_eq!(r.mesh.triangles.len(), 2);
        assert!((r.mesh.surface_area() - 4.0).abs() < 1e-6);
        // big endian
        let mut b = b"ply\nformat binary_big_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar uint vertex_indices\nend_header\n".to_vec();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                b.extend_from_slice(&c.to_be_bytes());
            }
        }
        b.push(3);
        for i in 0u32..3 {
            b.extend_from_slice(&i.to_be_bytes());
        }
        assert!((load(&b, "q.ply").mesh.surface_area() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn hostile_ply_files() {
        let good = ply_ascii();
        assert!(import_bytes(good.as_bytes(), "t.ply", &opts()).is_ok());
        for bad in [
            good.replace("element vertex 4", "element vertex 4000000"),
            good.replace("element vertex 4", "element vertex 99999999999999999999"),
            good.replace("element face 4", "element face 400000000"),
            good.replace("3 0 2 1", "3 0 2 9"),
            good.replace("3 0 2 1", "3 0 2 -1"),
            good.replace("3 0 2 1", "300000000 0 2 1"),
            good.replace("0 0 1 255", "0 0 nan 255"),
            good.replace("format ascii 1.0", "format wavy 1.0"),
            good.replace("property float z\n", ""),
            good.replace("end_header\n", ""),
            good.replace("property float x", "property blob x"),
            good.replace("0 1\n", ""),
            good.replace("ply\n", "plx\n"),
        ] {
            assert!(import_bytes(bad.as_bytes(), "t.ply", &opts()).is_err(), "{bad}");
        }
        // binary file cut off at every length
        let m = box_mesh([0.0; 3], [1.0; 3]);
        let bin = export_formats::ply(&surface(&m), "b").unwrap();
        assert!(import_bytes(&bin, "b.ply", &opts()).is_ok());
        for cut in 0..bin.len() {
            let _ = import_bytes(&bin[..cut], "b.ply", &opts());
        }
        // a list of 1,000,000 vertices per face in a tiny file
        let mut h = b"ply\nformat binary_little_endian 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uint int vi\nend_header\n".to_vec();
        h.extend_from_slice(&[0; 12]);
        h.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(import_bytes(&h, "t.ply", &opts()).is_err());
    }

    #[test]
    fn our_writers_round_trip_for_every_format() {
        let sphere = sphere_mesh(12.0, 16, 32);
        let v = sphere.signed_volume();
        let s = surface(&sphere);
        let stl = export_formats::stl_binary(&s.mesh, "s");
        let stla = export_formats::stl_ascii(&s.mesh, "s");
        let (obj, _) = export_formats::obj_files(&s, "s", "s.mtl").unwrap();
        let ply = export_formats::ply(&s, "s").unwrap();
        let mf = export_formats::threemf(&s, "s", Units::Mm).unwrap();
        let glb = export_formats::glb(&s, "s").unwrap();
        let files: Vec<(&str, Vec<u8>)> =
            vec![("s.stl", stl), ("s.stl", stla.into_bytes()), ("s.obj", obj.into_bytes()), ("s.ply", ply), ("s.3mf", mf), ("s.glb", glb)];
        for (name, bytes) in files {
            let r = import_bytes(&bytes, name, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.mesh.triangles.len(), sphere.triangles.len(), "{name}");
            assert_eq!(r.mesh.positions.len(), sphere.positions.len(), "{name}");
            assert!((r.mesh.signed_volume() - v).abs() / v < 1e-4, "{name}: {} vs {v}", r.mesh.signed_volume());
            assert!(r.health.is_closed(), "{name}");
        }
    }

    #[test]
    fn the_up_axis_choice_undoes_the_y_up_export() {
        let b = box_mesh([0.0, 0.0, 0.0], [10.0, 20.0, 30.0]);
        // what export does for "Y up": (x, y, z) -> (x, z, -y)
        let mut e = b.clone();
        for p in &mut e.positions {
            *p = [p[0], p[2], -p[1]];
        }
        let (obj, _) = export_formats::obj_files(&surface(&e), "b", "b.mtl").unwrap();
        let r = import_bytes(obj.as_bytes(), "b.obj", &ImportOptions::default()).unwrap();
        assert!(r.turned_y_up);
        assert_eq!(r.mesh.size(), [10.0, 20.0, 30.0]);
        assert!((r.mesh.signed_volume() - 6000.0).abs() < 1e-3);
        let r = import_bytes(obj.as_bytes(), "b.obj", &ImportOptions { y_up: Some(false) }).unwrap();
        assert!(!r.turned_y_up);
        assert_eq!(r.mesh.size(), [10.0, 30.0, 20.0]);
        // STL defaults to Z up
        let stl = export_formats::stl_binary(&to_trimesh(&b), "b");
        assert!(!import_bytes(&stl, "b.stl", &ImportOptions::default()).unwrap().turned_y_up);
    }

    #[test]
    fn weld_ground_scale_and_bounds() {
        // 12 loose triangles of a box (36 corners) weld to 8 points
        let b = box_mesh([-1.0, -2.0, 5.0], [3.0, 2.0, 9.0]);
        let loose = ImportedMesh {
            positions: b.triangles.iter().flatten().map(|i| b.positions[*i as usize]).collect(),
            triangles: (0..12).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect(),
        };
        let mut m = loose.clone();
        assert_eq!(m.weld(0.0), 0);
        assert_eq!(m.positions.len(), 8);
        assert!(m.health().is_closed());
        // a tolerance merges near points and drops the collapsed triangles
        let mut n = loose;
        n.positions[0][0] += 1e-5;
        assert_eq!(n.weld(0.0).min(1), 0);
        assert_eq!(n.positions.len(), 9);
        let mut n2 = n.clone();
        n2.weld(1e-3);
        assert_eq!(n2.positions.len(), 8);
        let mut z = ImportedMesh { positions: vec![[0.0; 3], [0.0001, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]] };
        assert_eq!(z.weld(0.01), 1);
        assert!(z.triangles.is_empty() && z.positions.is_empty());

        let mut m = b.clone();
        assert_eq!(m.bounds(), Some(([-1.0, -2.0, 5.0], [3.0, 2.0, 9.0])));
        m.centre_and_ground();
        assert_eq!(m.bounds(), Some(([-2.0, -2.0, 0.0], [2.0, 2.0, 4.0])));
        m.scale_to_size(40.0);
        assert_eq!(m.size(), [40.0, 40.0, 40.0]);
        let mut e = ImportedMesh::default();
        e.centre_and_ground();
        e.scale_to_size(10.0);
        assert_eq!(e.bounds(), None);
        let mut f = b.clone();
        f.scale_to_size(f32::NAN);
        f.scale(-3.0);
        assert_eq!(f, b);
    }

    /// #156: one reversed face of a closed box is reported, not called a normal closed surface.
    #[test]
    fn a_reversed_face_is_reported() {
        let mut b = box_mesh([0.0; 3], [1.0; 3]);
        assert_eq!(b.health().inconsistent_winding_edges, 0);
        if let Some(t) = b.triangles.first_mut() {
            t.swap(1, 2);
        }
        let h = b.health();
        assert_eq!((h.open_edges, h.inconsistent_winding_edges), (0, 3));
        for t in &mut b.triangles {
            t.swap(1, 2);
        }
        // everything reversed again but the first: still 3; all reversed together: 0
        let mut all = box_mesh([0.0; 3], [1.0; 3]);
        for t in &mut all.triangles {
            t.swap(1, 2);
        }
        assert_eq!(all.health().inconsistent_winding_edges, 0);
    }

    #[test]
    fn open_boxes_report_holes_and_flat_meshes_are_noted() {
        let mut b = box_mesh([0.0; 3], [1.0; 3]);
        b.triangles.truncate(10);
        let bytes = export_formats::stl_binary(&to_trimesh(&b), "o");
        let r = load(&bytes, "o.stl");
        assert_eq!(r.health.open_edges, 4);
        assert!(r.notes.iter().any(|n| n.contains("hole")));
        let flat = ImportedMesh { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], triangles: vec![[0, 1, 2]] };
        let r = load(&export_formats::stl_binary(&to_trimesh(&flat), "f"), "f.stl");
        assert!(r.notes.iter().any(|n| n.contains("Flat")));
    }

    #[test]
    fn unknown_and_oversized_input_is_refused() {
        assert!(import_bytes(b"hello world", "a.txt", &opts()).is_err());
        assert!(import_bytes(b"", "", &opts()).is_err());
        assert!(import_bytes(b"", "a.stl", &opts()).is_err());
        assert!(import_file(std::path::Path::new("/definitely/not/here.stl"), &opts()).is_err());
        assert_eq!(sniff(b"glTF....", "x.bin"), Some(MeshFormat::Glb));
        assert_eq!(sniff(b"PK\x03\x04", "x.zip"), Some(MeshFormat::ThreeMf));
        assert_eq!(sniff(b"solid a", ""), Some(MeshFormat::Stl));
        assert_eq!(sniff(b"v 1 2 3\n", ""), Some(MeshFormat::Obj));
        assert_eq!(sniff(b"\x00\x01", "A.OBJ"), Some(MeshFormat::Obj));
    }

    #[test]
    fn files_on_disk_load() {
        let dir = std::env::temp_dir().join(format!("meltcad-import-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("t.stl");
        std::fs::write(&p, ASCII_TETRA).unwrap();
        let r = import_file(&p, &opts()).unwrap();
        assert_eq!(r.format, MeshFormat::Stl);
        assert_eq!(r.mesh.triangles.len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Run with `--ignored --nocapture`: how long every reader takes for a big mesh.
    #[test]
    #[ignore = "timing report"]
    fn report_read_times_for_a_big_mesh() {
        use std::time::Instant;
        let m = sphere_mesh(100.0, 500, 500); // about 500k triangles
        let s = surface(&m);
        let files: Vec<(&str, Vec<u8>)> = vec![
            ("big.stl", export_formats::stl_binary(&s.mesh, "s")),
            ("big.stl", export_formats::stl_ascii(&s.mesh, "s").into_bytes()),
            ("big.obj", export_formats::obj_files(&s, "s", "s.mtl").unwrap().0.into_bytes()),
            ("big.ply", export_formats::ply(&s, "s").unwrap()),
            // (the test build caps the unpacked XML at 8 MB, so this one is a smaller mesh)
            ("big.3mf", export_formats::threemf(&surface(&sphere_mesh(100.0, 120, 120)), "s", Units::Mm).unwrap()),
            ("big.glb", export_formats::glb(&s, "s").unwrap()),
        ];
        for (name, bytes) in files {
            let t = Instant::now();
            let r = import_bytes(&bytes, name, &opts()).unwrap();
            println!("{name}: {} triangles, {:.1} MB read in {:.2}s", r.mesh.triangles.len(), bytes.len() as f64 / 1e6, t.elapsed().as_secs_f64());
        }
    }

    #[test]
    fn random_garbage_never_panics_in_any_reader() {
        let mut x = 7u32;
        let mut next = || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 16) as u8
        };
        let samples: Vec<Vec<u8>> = vec![
            ply_ascii().into_bytes(),
            ASCII_TETRA.as_bytes().to_vec(),
            export_formats::glb(&surface(&box_mesh([0.0; 3], [1.0; 3])), "g").unwrap(),
            export_formats::threemf(&surface(&box_mesh([0.0; 3], [1.0; 3])), "g", Units::Mm).unwrap(),
        ];
        for s in &samples {
            for _ in 0..300 {
                let mut c = s.clone();
                for _ in 0..1 + next() % 4 {
                    let i = (usize::from(next()) * 251 + usize::from(next())) % c.len();
                    c[i] = next();
                }
                for name in ["a.stl", "a.obj", "a.ply", "a.3mf", "a.glb"] {
                    let _ = import_bytes(&c, name, &opts());
                }
            }
        }
        for len in [0usize, 1, 3, 17, 100, 1000] {
            let junk: Vec<u8> = (0..len).map(|_| next()).collect();
            for name in ["a.stl", "a.obj", "a.ply", "a.3mf", "a.glb"] {
                let _ = import_bytes(&junk, name, &opts());
            }
        }
    }
}
