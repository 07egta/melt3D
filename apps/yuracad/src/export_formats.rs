//! File writers for export, all written from the public format descriptions: binary and ASCII STL,
//! Wavefront OBJ with an MTL file, 3MF (a ZIP holding the model XML, with base-material colours),
//! PLY with vertex colours, and binary glTF 2.0 (.glb) with vertex colours and PBR materials.

use std::fmt::Write as _;

use serde_json::json;
use solidcraft_geom::melt::TriMesh;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Stl,
    StlAscii,
    Obj,
    ThreeMf,
    Ply,
    Glb,
}

impl Format {
    pub const ALL: [Format; 6] = [Format::Stl, Format::ThreeMf, Format::Obj, Format::Ply, Format::Glb, Format::StlAscii];
    pub fn ext(self) -> &'static str {
        match self {
            Format::Stl | Format::StlAscii => "stl",
            Format::Obj => "obj",
            Format::ThreeMf => "3mf",
            Format::Ply => "ply",
            Format::Glb => "glb",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Format::Stl => "STL",
            Format::StlAscii => "STL(文字)",
            Format::Obj => "OBJ",
            Format::ThreeMf => "3MF",
            Format::Ply => "PLY",
            Format::Glb => "GLB",
        }
    }
    /// Can the file hold the colours of the model?
    pub fn has_colour(self) -> bool {
        !matches!(self, Format::Stl | Format::StlAscii)
    }
    /// glTF and OBJ pictures are usually Y up; printers and the other formats are Z up.
    pub fn y_up_by_default(self) -> bool {
        matches!(self, Format::Obj | Format::Glb)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Units {
    Mm,
    Cm,
    M,
    Inch,
}

impl Units {
    pub const ALL: [Units; 4] = [Units::Mm, Units::Cm, Units::M, Units::Inch];
    /// How many of this unit one millimetre is.
    pub fn per_mm(self) -> f64 {
        match self {
            Units::Mm => 1.0,
            Units::Cm => 0.1,
            Units::M => 0.001,
            Units::Inch => 1.0 / 25.4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Units::Mm => "mm",
            Units::Cm => "cm",
            Units::M => "m",
            Units::Inch => "inch",
        }
    }
    /// The 3MF `unit` attribute.
    pub fn threemf(self) -> &'static str {
        match self {
            Units::Mm => "millimeter",
            Units::Cm => "centimeter",
            Units::M => "meter",
            Units::Inch => "inch",
        }
    }
}

/// One look: colour (sRGB), roughness and metalness, each 0..=255.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mat8 {
    pub rgb: [u8; 3],
    pub rough: u8,
    pub metal: u8,
}

/// The colours of a surface. `palette` empty: no colour at all. One entry: the whole model has that
/// look. More: every triangle has a class (an index into the palette) and every vertex a smooth colour.
#[derive(Clone, Debug, Default)]
pub struct Look {
    pub palette: Vec<Mat8>,
    pub tri_class: Vec<u16>,
    pub vert_rgb: Vec<[u8; 3]>,
}

impl Look {
    pub fn none() -> Look {
        Look::default()
    }
    pub fn uniform(m: Mat8) -> Look {
        Look { palette: vec![m], tri_class: Vec::new(), vert_rgb: Vec::new() }
    }
    pub fn is_colour(&self) -> bool {
        !self.palette.is_empty()
    }
    fn class_of(&self, tri: usize) -> usize {
        self.tri_class.get(tri).map_or(0, |c| usize::from(*c))
    }
    fn mat(&self, class: usize) -> Mat8 {
        self.palette.get(class).copied().unwrap_or_default()
    }
    fn varies(&self) -> bool {
        self.palette.len() > 1 && !self.vert_rgb.is_empty()
    }
    /// The colour at a vertex: its own smooth colour, or the model's single one.
    fn vertex(&self, v: usize) -> [u8; 3] {
        if self.varies() { self.vert_rgb.get(v).copied().unwrap_or_default() } else { self.palette.first().map_or([200, 200, 200], |m| m.rgb) }
    }
}

/// A mesh ready to be written: coordinates already in the output units and axes.
pub struct Surface {
    pub mesh: TriMesh,
    pub look: Look,
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

/// A number with up to `dec` decimals and no trailing zeros.
fn num(x: f64, dec: usize) -> String {
    let mut s = format!("{:.*}", dec, fin(x));
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".into() } else { s }
}

fn tri_corners(mesh: &TriMesh) -> impl Iterator<Item = (usize, [[f64; 3]; 3])> + '_ {
    mesh.triangles.iter().enumerate().filter_map(|(i, t)| {
        let a = mesh.positions.get(t[0] as usize)?;
        let b = mesh.positions.get(t[1] as usize)?;
        let c = mesh.positions.get(t[2] as usize)?;
        Some((i, [*a, *b, *c]))
    })
}

fn facet_normal(t: &[[f64; 3]; 3]) -> [f64; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if l > 1e-30 { [c[0] / l, c[1] / l, c[2] / l] } else { [0.0, 0.0, 1.0] }
}

fn check(mesh: &TriMesh) -> Result<(), String> {
    if mesh.triangles.is_empty() || mesh.positions.is_empty() {
        return Err("there is nothing to write".into());
    }
    let n = mesh.positions.len();
    if n > u32::MAX as usize - 1 || mesh.triangles.len() > u32::MAX as usize / 4 {
        return Err("the model has too many triangles for this file".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- STL

/// A binary STL: 80-byte header, triangle count, then 50 bytes per triangle.
pub fn stl_binary(mesh: &TriMesh, name: &str) -> Vec<u8> {
    let count = tri_corners(mesh).take(u32::MAX as usize).count();
    let mut out = Vec::with_capacity(84 + 50 * count);
    // The header must not start with "solid", or readers take the file for ASCII STL.
    let mut header = format!("MELTCAD {name}").into_bytes();
    header.retain(|b| (0x20..0x7f).contains(b));
    header.resize(80, 0);
    out.extend_from_slice(&header);
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
    for (_, t) in tri_corners(mesh).take(u32::MAX as usize) {
        let n = facet_normal(&t);
        for x in n.iter().chain(t.iter().flatten()) {
            out.extend_from_slice(&(fin(*x) as f32).to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

/// A text STL.
pub fn stl_ascii(mesh: &TriMesh, name: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let clean = if clean.is_empty() { "MELTCAD".to_string() } else { clean };
    let mut out = format!("solid {clean}\n");
    for (_, t) in tri_corners(mesh) {
        let n = facet_normal(&t);
        let _ = writeln!(out, "facet normal {:.6} {:.6} {:.6}\n outer loop", n[0], n[1], n[2]);
        for p in &t {
            let _ = writeln!(out, "  vertex {:.7} {:.7} {:.7}", fin(p[0]), fin(p[1]), fin(p[2]));
        }
        out.push_str(" endloop\nendfacet\n");
    }
    let _ = writeln!(out, "endsolid {clean}");
    out
}

// ---------------------------------------------------------------- OBJ + MTL

fn mtl_name(class: usize) -> String {
    format!("yura_{class}")
}

/// A Wavefront OBJ and, when the model has colours, its MTL file. Faces are grouped by material.
pub fn obj_files(s: &Surface, name: &str, mtl_file: &str) -> Result<(String, Option<String>), String> {
    check(&s.mesh)?;
    let mesh = &s.mesh;
    let colour = s.look.is_colour();
    let mut out = format!("# MELTCAD\n# {} vertices, {} triangles\n", mesh.positions.len(), mesh.triangles.len());
    if colour {
        let _ = writeln!(out, "mtllib {mtl_file}");
    }
    let _ = writeln!(out, "o {}", name.replace(char::is_whitespace, "_"));
    for p in &mesh.positions {
        let _ = writeln!(out, "v {} {} {}", num(p[0], 6), num(p[1], 6), num(p[2], 6));
    }
    let n = mesh.positions.len() as u32;
    let ok = |t: &[u32; 3]| t.iter().all(|i| *i < n);
    if colour {
        let mut order: Vec<usize> = (0..mesh.triangles.len()).collect();
        order.sort_by_key(|i| s.look.class_of(*i));
        let mut current = usize::MAX;
        for i in order {
            let Some(t) = mesh.triangles.get(i) else { continue };
            if !ok(t) {
                continue;
            }
            let c = s.look.class_of(i);
            if c != current {
                let _ = writeln!(out, "usemtl {}", mtl_name(c));
                current = c;
            }
            let _ = writeln!(out, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
        }
    } else {
        for t in mesh.triangles.iter().filter(|t| ok(t)) {
            let _ = writeln!(out, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1);
        }
    }
    let mtl = colour.then(|| {
        let mut m = String::from("# MELTCAD materials\n");
        for (i, p) in s.look.palette.iter().enumerate() {
            let c = p.rgb.map(|v| f64::from(v) / 255.0);
            let rough = f64::from(p.rough) / 255.0;
            let metal = f64::from(p.metal) / 255.0;
            let ks = |k: f64| 0.04 + (k - 0.04) * metal;
            let _ = writeln!(
                m,
                "newmtl {}\nKa 0 0 0\nKd {:.4} {:.4} {:.4}\nKs {:.4} {:.4} {:.4}\nNs {:.1}\nd 1\nillum 2\nPr {:.3}\nPm {:.3}\n",
                mtl_name(i),
                c[0],
                c[1],
                c[2],
                ks(c[0]) * (1.0 - rough),
                ks(c[1]) * (1.0 - rough),
                ks(c[2]) * (1.0 - rough),
                (1.0 - rough) * 200.0 + 2.0,
                rough,
                metal
            );
        }
        m
    });
    Ok((out, mtl))
}

// ---------------------------------------------------------------- PLY

/// A binary little-endian PLY with vertex colours.
pub fn ply(s: &Surface, name: &str) -> Result<Vec<u8>, String> {
    check(&s.mesh)?;
    let colour = s.look.is_colour();
    let mesh = &s.mesh;
    let mut out = Vec::with_capacity(200 + mesh.positions.len() * 15 + mesh.triangles.len() * 13);
    let mut head = format!(
        "ply\nformat binary_little_endian 1.0\ncomment MELTCAD {}\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\n",
        name.replace(['\n', '\r'], " "),
        mesh.positions.len()
    );
    if colour {
        head.push_str("property uchar red\nproperty uchar green\nproperty uchar blue\n");
    }
    let _ = write!(head, "element face {}\nproperty list uchar int vertex_indices\nend_header\n", mesh.triangles.len());
    out.extend_from_slice(head.as_bytes());
    for (i, p) in mesh.positions.iter().enumerate() {
        for x in p {
            out.extend_from_slice(&(fin(*x) as f32).to_le_bytes());
        }
        if colour {
            out.extend_from_slice(&s.look.vertex(i));
        }
    }
    let n = mesh.positions.len() as u32;
    // a triangle with a broken index is written as a degenerate one so the count in the header holds
    for t in &mesh.triangles {
        out.push(3);
        for i in t {
            out.extend_from_slice(&(if *i < n { *i } else { 0 } as i32).to_le_bytes());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- 3MF

fn xml_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c if (c as u32) < 0x20 => " ".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// A 3MF file (what slicers like best): a ZIP with the model as XML. Colours go in as a base
/// materials group, one `<base>` per look and a `p1` index on each triangle.
pub fn threemf(s: &Surface, name: &str, unit: Units) -> Result<Vec<u8>, String> {
    check(&s.mesh)?;
    let mesh = &s.mesh;
    let mut model = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"{}\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n<metadata name=\"Title\">{}</metadata>\n<metadata name=\"Application\">MELTCAD</metadata>\n<resources>\n",
        unit.threemf(),
        xml_escape(name)
    );
    let colour = s.look.is_colour();
    // the most common look is the object's default, so its triangles need no attribute
    let mut main = 0usize;
    if colour && s.look.palette.len() > 1 {
        let mut count = vec![0usize; s.look.palette.len()];
        for i in 0..mesh.triangles.len() {
            if let Some(c) = count.get_mut(s.look.class_of(i)) {
                *c += 1;
            }
        }
        main = count.iter().enumerate().fold((0usize, 0usize), |best, (i, c)| if *c > best.1 { (i, *c) } else { best }).0;
    }
    if colour {
        model.push_str("<basematerials id=\"1\">\n");
        for (i, p) in s.look.palette.iter().enumerate() {
            let _ = writeln!(model, "<base name=\"yura_{i}\" displaycolor=\"#{:02X}{:02X}{:02X}\"/>", p.rgb[0], p.rgb[1], p.rgb[2]);
        }
        model.push_str("</basematerials>\n");
        let _ = writeln!(model, "<object id=\"2\" type=\"model\" pid=\"1\" pindex=\"{main}\">");
    } else {
        model.push_str("<object id=\"2\" type=\"model\">\n");
    }
    model.push_str("<mesh>\n<vertices>\n");
    for p in &mesh.positions {
        let _ = writeln!(model, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", num(p[0], 6), num(p[1], 6), num(p[2], 6));
    }
    model.push_str("</vertices>\n<triangles>\n");
    let n = mesh.positions.len() as u32;
    for (i, t) in mesh.triangles.iter().enumerate().filter(|(_, t)| t.iter().all(|v| *v < n)) {
        let c = s.look.class_of(i);
        if colour && c != main {
            let _ = writeln!(model, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{c}\"/>", t[0], t[1], t[2]);
        } else {
            let _ = writeln!(model, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2]);
        }
    }
    model.push_str("</triangles>\n</mesh>\n</object>\n</resources>\n<build>\n<item objectid=\"2\"/>\n</build>\n</model>\n");
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>";
    zip(&[("[Content_Types].xml", types.as_bytes()), ("_rels/.rels", rels.as_bytes()), ("3D/3dmodel.model", model.as_bytes())])
}

pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
            *slot = c;
        }
        t
    });
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc = table.get(((crc ^ u32::from(*b)) & 0xFF) as usize).copied().unwrap_or(0) ^ (crc >> 8);
    }
    !crc
}

/// A ZIP archive. Small files are stored as they are; big ones are deflated (every reader accepts both).
fn zip(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let too_big = || "the model is too big for a 3MF file".to_string();
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in files {
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let nlen = u16::try_from(name.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        let packed = if data.len() > 4096 { Some(crate::export_deflate::deflate(data)).filter(|p| p.len() < data.len()) } else { None };
        let (method, body): (u16, &[u8]) = match &packed {
            Some(p) => (8, p),
            None => (0, data),
        };
        let csize = u32::try_from(body.len()).map_err(|_| too_big())?;
        let mut head = Vec::new();
        head.extend_from_slice(&[20, 0, 0, 0]); // version, flags
        head.extend_from_slice(&method.to_le_bytes());
        head.extend_from_slice(&[0, 0, 0x21, 0]); // time, date (1980-01-01)
        head.extend_from_slice(&crc.to_le_bytes());
        head.extend_from_slice(&csize.to_le_bytes());
        head.extend_from_slice(&size.to_le_bytes());
        head.extend_from_slice(&nlen.to_le_bytes());
        head.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&head);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&[20, 0]);
        central.extend_from_slice(&head);
        central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // comment length, disk, internal and external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_big())?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    let count = u16::try_from(files.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    Ok(out)
}

// ---------------------------------------------------------------- GLB

fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// A binary glTF 2.0 file: one mesh, vertex colours (linear, as the format wants) and a PBR material
/// for each distinct roughness and metalness. Glossy and rough parts are separate primitives that
/// share the vertices.
pub fn glb(s: &Surface, name: &str) -> Result<Vec<u8>, String> {
    check(&s.mesh)?;
    let mesh = &s.mesh;
    let n = mesh.positions.len();
    let mut bin: Vec<u8> = Vec::with_capacity(n * 24 + mesh.triangles.len() * 12);
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &mesh.positions {
        for a in 0..3 {
            let v = fin(p[a]) as f32;
            lo[a] = lo[a].min(v);
            hi[a] = hi[a].max(v);
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    let pos_len = bin.len();
    let varying = s.look.varies();
    if varying {
        for i in 0..n {
            for c in s.look.vertex(i) {
                bin.extend_from_slice(&srgb_to_linear(c).to_le_bytes());
            }
        }
    }
    let col_len = bin.len() - pos_len;
    // group the triangles by (roughness, metalness)
    let mut groups: Vec<((u8, u8), Mat8, Vec<u32>)> = Vec::new();
    let nv = n as u32;
    for (i, t) in mesh.triangles.iter().enumerate() {
        if t.iter().any(|v| *v >= nv) {
            continue;
        }
        let m = if s.look.is_colour() { s.look.mat(s.look.class_of(i)) } else { Mat8 { rgb: [200, 200, 200], rough: 128, metal: 0 } };
        let key = (m.rough / 26, m.metal / 26);
        let slot = match groups.iter().position(|g| g.0 == key) {
            Some(k) => k,
            None => {
                groups.push((key, m, Vec::new()));
                groups.len() - 1
            }
        };
        if let Some(g) = groups.get_mut(slot) {
            g.2.extend_from_slice(t);
        }
    }
    let mut views = vec![json!({"buffer": 0, "byteOffset": 0, "byteLength": pos_len, "target": 34962})];
    let mut accessors = vec![json!({"bufferView": 0, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi})];
    let mut attributes = serde_json::Map::new();
    attributes.insert("POSITION".into(), json!(0));
    if varying {
        views.push(json!({"buffer": 0, "byteOffset": pos_len, "byteLength": col_len, "target": 34962}));
        accessors.push(json!({"bufferView": 1, "componentType": 5126, "count": n, "type": "VEC3"}));
        attributes.insert("COLOR_0".into(), json!(1));
    }
    let mut materials = Vec::new();
    let mut primitives = Vec::new();
    for (_, m, idx) in &groups {
        let offset = bin.len();
        for i in idx {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let view = views.len();
        views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": idx.len() * 4, "target": 34963}));
        let acc = accessors.len();
        accessors.push(json!({"bufferView": view, "componentType": 5125, "count": idx.len(), "type": "SCALAR"}));
        let base = if varying { [1.0, 1.0, 1.0, 1.0] } else { [srgb_to_linear(m.rgb[0]), srgb_to_linear(m.rgb[1]), srgb_to_linear(m.rgb[2]), 1.0] };
        materials.push(json!({
            "name": format!("yura_{}", materials.len()),
            "pbrMetallicRoughness": {"baseColorFactor": base, "metallicFactor": f32::from(m.metal) / 255.0, "roughnessFactor": f32::from(m.rough) / 255.0},
            "doubleSided": false
        }));
        primitives.push(json!({"attributes": attributes, "indices": acc, "material": materials.len() - 1, "mode": 4}));
    }
    if primitives.is_empty() {
        return Err("there is nothing to write".into());
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "MELTCAD"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": name}],
        "meshes": [{"name": name, "primitives": primitives}],
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}]
    });
    let mut text = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let total32 = u32::try_from(total).map_err(|_| "the model is too big for a GLB file".to_string())?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total32.to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    Ok(out)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A tetrahedron-ish closed mesh with 4 triangles.
    pub fn tetra(scale: f64) -> TriMesh {
        TriMesh {
            positions: vec![[0.0, 0.0, 0.0], [scale, 0.0, 0.0], [0.0, scale, 0.0], [0.0, 0.0, scale]],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    pub fn two_colours() -> Look {
        Look {
            palette: vec![Mat8 { rgb: [255, 0, 0], rough: 100, metal: 0 }, Mat8 { rgb: [0, 0, 255], rough: 20, metal: 255 }],
            tri_class: vec![0, 0, 1, 1],
            vert_rgb: vec![[255, 0, 0], [255, 0, 0], [0, 0, 255], [0, 0, 255]],
        }
    }

    pub fn read_stl_binary(b: &[u8]) -> Vec<[[f32; 3]; 3]> {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        assert_eq!(b.len(), 84 + 50 * n);
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        (0..n)
            .map(|i| {
                let o = 84 + 50 * i + 12;
                [[f(o), f(o + 4), f(o + 8)], [f(o + 12), f(o + 16), f(o + 20)], [f(o + 24), f(o + 28), f(o + 32)]]
            })
            .collect()
    }

    /// The files of a ZIP written with `zip`, by name (reads stored and deflated entries).
    pub fn unzip(b: &[u8]) -> Vec<(String, Vec<u8>)> {
        let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        let u32at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
        let eocd = (0..b.len().saturating_sub(21)).rev().find(|o| &b[*o..*o + 4] == b"PK\x05\x06").expect("end of central directory");
        let count = u16at(eocd + 10);
        let mut off = u32at(eocd + 16);
        let mut files = Vec::new();
        for _ in 0..count {
            assert_eq!(u32at(off), 0x0201_4b50);
            let method = u16at(off + 10);
            let crc = u32at(off + 16);
            let csize = u32at(off + 20);
            let usize_ = u32at(off + 24);
            let nlen = u16at(off + 28);
            let xlen = u16at(off + 30);
            let clen = u16at(off + 32);
            let local = u32at(off + 42);
            let name = String::from_utf8(b[off + 46..off + 46 + nlen].to_vec()).unwrap();
            assert_eq!(u32at(local), 0x0403_4b50);
            let ln = u16at(local + 26);
            let lx = u16at(local + 28);
            let start = local + 30 + ln + lx;
            let body = &b[start..start + csize];
            let data = if method == 0 {
                body.to_vec()
            } else {
                assert_eq!(method, 8);
                crate::export_deflate::tests::inflate(body)
            };
            assert_eq!(data.len(), usize_);
            assert_eq!(crc32(&data) as usize, crc);
            files.push((name, data));
            off += 46 + nlen + xlen + clen;
        }
        files
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn binary_stl_round_trips() {
        let m = tetra(10.0);
        let b = stl_binary(&m, "a b");
        assert!(!b.starts_with(b"solid"));
        let t = read_stl_binary(&b);
        assert_eq!(t.len(), 4);
        assert_eq!(t[3], [[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]]);
        // the stored normal of the first facet points down (-z)
        let nz = f32::from_le_bytes([b[84 + 8], b[84 + 9], b[84 + 10], b[84 + 11]]);
        assert!((nz + 1.0).abs() < 1e-6);
    }

    #[test]
    fn ascii_stl_has_one_facet_per_triangle() {
        let m = tetra(2.5);
        let s = stl_ascii(&m, "my model!");
        assert!(s.starts_with("solid my_model_\n") && s.trim_end().ends_with("endsolid my_model_"));
        assert_eq!(s.matches("facet normal").count(), 4);
        assert_eq!(s.matches("endfacet").count(), 4);
        let verts: Vec<Vec<f64>> = s
            .lines()
            .filter(|l| l.trim_start().starts_with("vertex"))
            .map(|l| l.split_whitespace().skip(1).map(|x| x.parse().unwrap()).collect())
            .collect();
        assert_eq!(verts.len(), 12);
        assert_eq!(verts[9], vec![2.5, 0.0, 0.0]);
    }

    #[test]
    fn obj_has_vertices_faces_and_materials_with_the_right_colours() {
        let s = Surface { mesh: tetra(10.0), look: two_colours() };
        let (obj, mtl) = obj_files(&s, "a b", "x.mtl").unwrap();
        let mtl = mtl.unwrap();
        assert_eq!(obj.lines().filter(|l| l.starts_with("v ")).count(), 4);
        assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 4);
        assert!(obj.contains("mtllib x.mtl") && obj.contains("usemtl yura_0") && obj.contains("usemtl yura_1"));
        assert!(
            obj.lines().filter(|l| l.starts_with("f ")).all(|l| l.split(' ').skip(1).all(|i| i.parse::<usize>().is_ok_and(|i| (1..=4).contains(&i))))
        );
        assert!(mtl.contains("newmtl yura_0\n") && mtl.contains("Kd 1.0000 0.0000 0.0000"));
        assert!(mtl.contains("Kd 0.0000 0.0000 1.0000") && mtl.contains("Pm 1.000"));
        // no colours: no mtl, no mtllib
        let (obj, mtl) = obj_files(&Surface { mesh: tetra(1.0), look: Look::none() }, "a", "x.mtl").unwrap();
        assert!(mtl.is_none() && !obj.contains("mtllib") && !obj.contains("usemtl"));
    }

    #[test]
    fn ply_has_colours_and_counts() {
        let s = Surface { mesh: tetra(10.0), look: two_colours() };
        let b = ply(&s, "t").unwrap();
        let end = b.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
        let head = String::from_utf8(b[..end].to_vec()).unwrap();
        assert!(head.contains("element vertex 4") && head.contains("element face 4") && head.contains("property uchar red"));
        let body = &b[end..];
        assert_eq!(body.len(), 4 * 15 + 4 * 13);
        // vertex 2 is blue
        assert_eq!(&body[2 * 15 + 12..2 * 15 + 15], &[0, 0, 255]);
        // the last face
        let f = &body[4 * 15 + 3 * 13..];
        assert_eq!(f[0], 3);
        assert_eq!(i32::from_le_bytes([f[1], f[2], f[3], f[4]]), 1);
        // without colour there are no colour properties
        let b = ply(&Surface { mesh: tetra(1.0), look: Look::none() }, "t").unwrap();
        assert!(!String::from_utf8_lossy(&b).contains("red"));
    }

    #[test]
    fn threemf_holds_units_vertices_triangles_and_colours() {
        let s = Surface { mesh: tetra(10.0), look: two_colours() };
        let z = threemf(&s, "a<b>", Units::Cm).unwrap();
        let files = unzip(&z);
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]);
        let xml = String::from_utf8(files[2].1.clone()).unwrap();
        assert!(xml.contains("unit=\"centimeter\"") && xml.contains("a&lt;b&gt;"));
        assert_eq!(xml.matches("<vertex ").count(), 4);
        assert_eq!(xml.matches("<triangle ").count(), 4);
        assert!(xml.contains("<base name=\"yura_0\" displaycolor=\"#FF0000\"/>") && xml.contains("displaycolor=\"#0000FF\""));
        // the common look (class 0, two of four) is the object's default, the other is set on its triangles
        assert!(xml.contains("pid=\"1\" pindex=\"0\""));
        assert_eq!(xml.matches("p1=\"1\"").count(), 2);
        assert!(xml.contains("<vertex x=\"10\" y=\"0\" z=\"0\"/>"));
        // no colour: no materials
        let z = threemf(&Surface { mesh: tetra(1.0), look: Look::none() }, "a", Units::Mm).unwrap();
        let xml = String::from_utf8(unzip(&z)[2].1.clone()).unwrap();
        assert!(!xml.contains("basematerials") && xml.contains("unit=\"millimeter\""));
        // an empty mesh is refused
        assert!(threemf(&Surface { mesh: TriMesh::default(), look: Look::none() }, "a", Units::Mm).is_err());
    }

    fn glb_json(b: &[u8]) -> (serde_json::Value, Vec<u8>) {
        assert_eq!(&b[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 2);
        assert_eq!(u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as usize, b.len());
        let jl = u32::from_le_bytes([b[12], b[13], b[14], b[15]]) as usize;
        assert_eq!(&b[16..20], b"JSON");
        let doc: serde_json::Value = serde_json::from_slice(&b[20..20 + jl]).unwrap();
        let bo = 20 + jl;
        let bl = u32::from_le_bytes([b[bo], b[bo + 1], b[bo + 2], b[bo + 3]]) as usize;
        assert_eq!(&b[bo + 4..bo + 8], b"BIN\0");
        assert_eq!(bo + 8 + bl, b.len());
        assert_eq!(jl % 4, 0);
        assert_eq!(bl % 4, 0);
        (doc, b[bo + 8..].to_vec())
    }

    #[test]
    fn glb_has_positions_colours_indices_and_materials() {
        let s = Surface { mesh: tetra(10.0), look: two_colours() };
        let b = glb(&s, "t").unwrap();
        let (doc, bin) = glb_json(&b);
        assert_eq!(doc["asset"]["version"], "2.0");
        let acc = &doc["accessors"];
        assert_eq!(acc[0]["count"], 4);
        assert_eq!(acc[0]["max"], json!([10.0, 10.0, 10.0]));
        assert_eq!(acc[0]["min"], json!([0.0, 0.0, 0.0]));
        // two (roughness, metal) groups: two primitives and two materials sharing the vertices
        let prims = doc["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(prims.len(), 2);
        assert_eq!(doc["materials"].as_array().unwrap().len(), 2);
        assert!(prims.iter().all(|p| p["attributes"]["POSITION"] == 0 && p["attributes"]["COLOR_0"] == 1));
        let metal = doc["materials"].as_array().unwrap().iter().filter(|m| m["pbrMetallicRoughness"]["metallicFactor"] == 1.0).count();
        assert_eq!(metal, 1);
        // read the third vertex position and the second vertex colour back from the binary chunk
        let f = |o: usize| f32::from_le_bytes([bin[o], bin[o + 1], bin[o + 2], bin[o + 3]]);
        assert_eq!((f(2 * 12), f(2 * 12 + 4), f(2 * 12 + 8)), (0.0, 10.0, 0.0));
        let col = doc["bufferViews"][1]["byteOffset"].as_u64().unwrap() as usize;
        assert_eq!((f(col + 3 * 12), f(col + 3 * 12 + 4), f(col + 3 * 12 + 8)), (0.0, 0.0, 1.0));
        // the indices of both primitives together are the 4 triangles
        let total: u64 = prims.iter().map(|p| acc[p["indices"].as_u64().unwrap() as usize]["count"].as_u64().unwrap()).sum();
        assert_eq!(total, 12);
        // a single-look model has one material with its colour and no vertex colours
        let b = glb(&Surface { mesh: tetra(10.0), look: Look::uniform(Mat8 { rgb: [255, 255, 255], rough: 128, metal: 0 }) }, "t").unwrap();
        let (doc, _) = glb_json(&b);
        assert!(doc["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"].is_null());
        assert_eq!(doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"], json!([1.0, 1.0, 1.0, 1.0]));
        assert!(glb(&Surface { mesh: TriMesh::default(), look: Look::none() }, "t").is_err());
    }

    #[test]
    fn numbers_lose_their_trailing_zeros() {
        assert_eq!(num(10.0, 6), "10");
        assert_eq!(num(-0.0000001, 6), "0");
        assert_eq!(num(1.25, 6), "1.25");
        assert_eq!(num(f64::NAN, 6), "0");
    }
}
