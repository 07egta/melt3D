//! SVG import: our own small parser. An SVG picture becomes a list of closed outlines
//! (solids and holes), and the outlines become `Extrude` shapes in one new group.
//!
//! Supported: `<path>` (all commands, curves and arcs are flattened), `<polygon>`, `<polyline>`,
//! `<rect>` (with rx/ry), `<circle>`, `<ellipse>`, nested `<g>`, `transform`, `viewBox`, `width` and
//! `height` with units, `fill-rule`, `display:none`, and local `<use>` references including symbols.
//! Strokes, text, images and gradients are reported as unsupported. Hostile files never panic:
//! the size, the number of commands, visits, points, outlines and
//! the nesting are all capped.

use std::collections::HashMap;

use crate::model::{Group, Kind, MAX_OUTLINE, Material, Op, Shape};

/// A bigger file is refused.
pub const MAX_BYTES: usize = 2_000_000;
const MAX_COMMANDS: usize = 20_000;
const MAX_POINTS: usize = 400_000;
const MAX_ELEMENTS: usize = 6_000;
/// Includes empty groups and repeated references, which may generate no geometry at all.
const MAX_VISITS: usize = MAX_ELEMENTS * 4;
/// Bytes of referenced fragments scanned in one pass. Repeated `<use>` of a large definition
/// (even one that is mostly comments or style text) is bounded by this, not only by tag visits.
const MAX_REFERENCE_BYTES: usize = MAX_BYTES * 16;
const MAX_DEPTH: usize = 64;
/// Outlines kept after throwing out the tiniest ones.
const MAX_CONTOURS: usize = 300;
/// How deep outlines may sit inside each other.
const MAX_NEST: usize = 32;
/// Curves are flattened for a picture this big (mm); the artwork is only rescaled afterwards.
const REF_MM: f64 = 60.0;
const FLAT_TOL_MM: f64 = 0.05;

/// A star and a ring (a circle with a hole), for trying the importer without a file.
pub const SAMPLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 80" width="160mm" height="80mm">
  <path fill="#f4a261" d="M40 6 L48.5 29 L73 29.8 L53.7 45 L60.4 68.5 L40 55 L19.6 68.5 L26.3 45 L7 29.8 L31.5 29 Z"/>
  <path fill="#7aa2f7" fill-rule="evenodd" d="M120 8 a32 32 0 1 0 0.001 0 Z M120 26 a14 14 0 1 1 -0.001 0 Z"/>
</svg>"##;

/// One closed outline.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    /// Corners, counter-clockwise, at most `MAX_OUTLINE`.
    pub pts: Vec<[f32; 2]>,
    /// A hole is carved out of the solid around it.
    pub hole: bool,
    pub color: Option<[f32; 3]>,
}

/// A parsed picture: centred at the origin, Y up, its longest side = 1. Scale it with `placed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Artwork {
    /// Solids first, each followed by its holes.
    pub contours: Vec<Contour>,
    /// Width and height of the picture when its longest side is 1.
    pub aspect: [f32; 2],
    /// The picture's own size in mm (96 dpi for px).
    pub natural_mm: [f32; 2],
    /// How many outlines were thrown away (too tiny, too many, too deeply nested).
    pub dropped: usize,
    /// Nonfatal omissions: the user must not mistake a partial import for an exact SVG.
    pub warnings: Vec<String>,
}

impl Artwork {
    /// The outlines in mm with the longest side = `target_mm`. `flip` turns the picture upside down.
    pub fn placed(&self, target_mm: f32, flip: bool) -> Vec<Contour> {
        let t = if target_mm.is_finite() { target_mm.clamp(0.5, 500.0) } else { 60.0 };
        let mut out = Vec::with_capacity(self.contours.len());
        for c in &self.contours {
            let mut pts: Vec<[f32; 2]> = c.pts.iter().map(|p| [p[0] * t, if flip { -p[1] * t } else { p[1] * t }]).collect();
            if area32(&pts) < 0.0 {
                pts.reverse();
            }
            out.push(Contour { pts, hole: c.hole, color: c.color });
        }
        out
    }
    pub fn solids(&self) -> usize {
        self.contours.iter().filter(|c| !c.hole).count()
    }
    pub fn holes(&self) -> usize {
        self.contours.iter().filter(|c| c.hole).count()
    }
}

/// Read an `.svg` file (refusing anything over 2 MB).
pub fn load_file(path: &std::path::Path) -> Result<Artwork, String> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    f.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    parse_bytes(&bytes)
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Artwork, String> {
    if bytes.len() > MAX_BYTES {
        return Err("File is too big (up to 2 MB)".into());
    }
    parse(&String::from_utf8_lossy(bytes))
}

/// Parse SVG text into outlines.
pub fn parse(text: &str) -> Result<Artwork, String> {
    if text.len() > MAX_BYTES {
        return Err("File is too big (up to 2 MB)".into());
    }
    let mut pos = 0;
    if next_tag(text, &mut pos).is_none_or(|tag| tag.closing || tag.name != "svg") {
        return Err("This doesn't look like an SVG file".into());
    }
    let ids = collect_ids(text);
    let mut limited = false;
    let mut unresolved = false;
    // pass 1: rough, only to learn the size of the picture
    let rough = walk(text, None, &ids, &mut limited, &mut unresolved);
    let Some((lo, hi)) = bounds(rough.iter().flat_map(|r| r.polys.iter().flatten())) else {
        if unresolved {
            return Err("A referenced SVG shape is missing or refers to itself. Expand the references into shapes in your SVG editor".into());
        }
        return Err("No filled shape found (line-only drawings can't be read)".into());
    };
    let side = (hi.0 - lo.0).max(hi.1 - lo.1);
    if side.is_nan() || side <= 1e-9 || !side.is_finite() {
        return Err("Shape is too small to read".into());
    }
    // pass 2: curves flattened to ~0.05 mm for a 60 mm picture
    let tol = FLAT_TOL_MM / (REF_MM / side);
    let raws = walk(text, Some(tol), &ids, &mut limited, &mut unresolved);
    let mut art = build_artwork(&raws, side)?;
    art.warnings = unsupported_features(text);
    if limited {
        art.warnings.push("SVG complexity limit reached; some details were not loaded".into());
    }
    if unresolved {
        art.warnings.push("Skipped SVG references whose target is missing or refers to itself".into());
    }
    Ok(art)
}

/// Report features we deliberately do not rasterize or convert into geometry.
/// This pass parses only tags; it never resolves external resources.
fn unsupported_features(src: &str) -> Vec<String> {
    let mut tags = std::collections::BTreeSet::new();
    let mut pos = 0;
    let mut scanned = 0;
    while let Some(tag) = next_tag(src, &mut pos) {
        scanned += 1;
        if scanned > MAX_ELEMENTS * 2 {
            break;
        }
        if tag.closing {
            continue;
        }
        match tag.name.as_str() {
            "text" | "image" | "foreignobject" | "filter" | "clippath" | "mask" | "lineargradient" | "radialgradient" | "pattern" | "marker"
            | "style" => {
                tags.insert(tag.name.clone());
            }
            "use" if tag.attr("href").or_else(|| tag.attr("xlink:href")).is_some_and(|s| !s.starts_with('#')) => {
                tags.insert("external use".into());
            }
            _ => {}
        }
        if tag.prop("stroke").is_some_and(|s| s != "none") {
            tags.insert("stroke".into());
        }
    }
    tags.into_iter().map(|t| format!("Unsupported SVG element or setting: {t} (not loaded)")).collect()
}

// ---------------------------------------------------------------------------------------------
// geometry helpers

#[derive(Clone, Copy, Debug)]
struct Affine([f64; 6]);

impl Affine {
    const ID: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    /// `self` after `o` (o is applied to a point first).
    fn mul(&self, o: &Affine) -> Affine {
        let (s, o) = (&self.0, &o.0);
        Affine([
            s[0] * o[0] + s[2] * o[1],
            s[1] * o[0] + s[3] * o[1],
            s[0] * o[2] + s[2] * o[3],
            s[1] * o[2] + s[3] * o[3],
            s[0] * o[4] + s[2] * o[5] + s[4],
            s[1] * o[4] + s[3] * o[5] + s[5],
        ])
    }
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
    /// Largest singular value: a conservative length scale even under skew.
    fn scale(&self) -> f64 {
        let m = &self.0;
        let a = m[0] * m[0] + m[1] * m[1];
        let b = m[0] * m[2] + m[1] * m[3];
        let c = m[2] * m[2] + m[3] * m[3];
        ((a + c + (a - c).hypot(2.0 * b)) * 0.5).sqrt()
    }
    fn ok(&self) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() < 1e9)
    }
}

fn bounds<'a>(pts: impl Iterator<Item = &'a (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut any = false;
    for p in pts {
        any = true;
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    any.then_some((lo, hi))
}

/// Signed area (counter-clockwise, Y up = positive).
fn area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let (q, r) = (p[i], p[(i + 1) % n]);
        a += q.0 * r.1 - r.0 * q.1;
    }
    a * 0.5
}

fn area32(p: &[[f32; 2]]) -> f32 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let (q, r) = (p[i], p[(i + 1) % n]);
        a += q[0] * r[1] - r[0] * q[1];
    }
    a * 0.5
}

fn inside(poly: &[(f64, f64)], pt: (f64, f64)) -> bool {
    let n = poly.len();
    let mut c = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > pt.1) != (b.1 > pt.1) && pt.0 < (b.0 - a.0) * (pt.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
        j = i;
    }
    c
}

/// The part of `subject` inside the convex polygon `clip` (Sutherland–Hodgman). The result can be
/// empty or a line when the outline only touches the region.
fn clip_polygon(subject: &[(f64, f64)], clip: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if clip.len() < 3 {
        return subject.to_vec();
    }
    let sign = if area(clip) < 0.0 { -1.0 } else { 1.0 };
    let mut out = subject.to_vec();
    for i in 0..clip.len() {
        if out.is_empty() {
            break;
        }
        let a = clip[i];
        let b = clip[(i + 1) % clip.len()];
        // Positive on the inside of this edge; the sign makes the test independent of winding.
        let side = |p: (f64, f64)| sign * ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0));
        let mut next = Vec::with_capacity(out.len() + 4);
        for j in 0..out.len() {
            let p = out[j];
            let q = out[(j + 1) % out.len()];
            let (sp, sq) = (side(p), side(q));
            let (pin, qin) = (sp >= 0.0, sq >= 0.0);
            if pin {
                next.push(p);
            }
            if pin != qin {
                let d = sp - sq;
                if d != 0.0 {
                    let t = (sp / d).clamp(0.0, 1.0);
                    next.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
                }
            }
        }
        out = next;
    }
    // A corner exactly on the clip edge can be emitted twice: drop repeats so simplify() sees a
    // clean outline.
    let mut clean: Vec<(f64, f64)> = Vec::with_capacity(out.len());
    for p in out {
        if clean.last().is_none_or(|q| (q.0 - p.0).abs() > 1e-9 || (q.1 - p.1).abs() > 1e-9) {
            clean.push(p);
        }
    }
    if clean.len() > 1 && clean.first().zip(clean.last()).is_some_and(|(a, b)| (a.0 - b.0).abs() <= 1e-9 && (a.1 - b.1).abs() <= 1e-9) {
        clean.pop();
    }
    clean
}

/// A point that is surely inside the outline (even a concave one).
fn interior_point(p: &[(f64, f64)]) -> (f64, f64) {
    let n = p.len().max(1) as f64;
    let c = (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n);
    if inside(p, c) {
        return c;
    }
    let mut ys: Vec<f64> = p.iter().map(|q| q.1).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut best: Option<(f64, (f64, f64))> = None;
    for k in [2usize, 3, 4, 1, 5] {
        let (i, j) = ((ys.len() * k / 6).min(ys.len().saturating_sub(1)), (ys.len() * k / 6 + 1).min(ys.len().saturating_sub(1)));
        let (Some(a), Some(b)) = (ys.get(i), ys.get(j)) else { continue };
        let y = (a + b) * 0.5;
        let mut xs = Vec::new();
        for i in 0..p.len() {
            let (q, r) = (p[i], p[(i + 1) % p.len()]);
            if (q.1 > y) != (r.1 > y) {
                xs.push(q.0 + (y - q.1) / (r.1 - q.1) * (r.0 - q.0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for w in xs.as_chunks::<2>().0.iter() {
            let span = w[1] - w[0];
            if best.is_none_or(|(s, _)| span > s) {
                best = Some((span, ((w[0] + w[1]) * 0.5, y)));
            }
        }
    }
    best.map(|b| b.1).unwrap_or(p.first().copied().unwrap_or((0.0, 0.0)))
}

/// Ramer-Douglas-Peucker on a closed outline: which points stay.
fn rdp_closed(p: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    let n = p.len();
    if n <= 3 {
        return p.to_vec();
    }
    // anchor on the point farthest from point 0, so both halves are open chains
    let far = (1..n)
        .max_by(|&a, &b| {
            let d = |i: usize| (p[i].0 - p[0].0).powi(2) + (p[i].1 - p[0].1).powi(2);
            d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(1);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let mut stack = vec![(0usize, far), (far, n)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (p[a], p[b % n]);
        let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
        let len2 = dx * dx + dy * dy;
        let mut worst = (0.0_f64, 0usize);
        for (i, q) in p.iter().enumerate().take(b).skip(a + 1) {
            let t = if len2 < 1e-30 { 0.0 } else { (((q.0 - pa.0) * dx + (q.1 - pa.1) * dy) / len2).clamp(0.0, 1.0) };
            let d = (q.0 - pa.0 - t * dx).hypot(q.1 - pa.1 - t * dy);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol && worst.1 > a {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    p.iter().zip(&keep).filter(|(_, k)| **k).map(|(q, _)| *q).collect()
}

/// At most `MAX_OUTLINE` corners, with the smallest error that gets there.
fn simplify(p: &[(f64, f64)], side: f64) -> Vec<(f64, f64)> {
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(p.len());
    for &q in p {
        if pts.last().is_none_or(|l| (l.0 - q.0).hypot(l.1 - q.1) > side * 1e-7) {
            pts.push(q);
        }
    }
    while pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| (a.0 - b.0).hypot(a.1 - b.1) <= side * 1e-7) {
        pts.pop();
    }
    // very long outlines are thinned first so the work stays small
    if pts.len() > 2048 {
        let step = pts.len().div_ceil(2048);
        pts = pts.iter().step_by(step).copied().collect();
    }
    let mut tol = side * 2e-4;
    for _ in 0..40 {
        let r = rdp_closed(&pts, tol);
        if r.len() <= MAX_OUTLINE {
            return r;
        }
        tol *= 1.6;
    }
    let step = pts.len().div_ceil(MAX_OUTLINE).max(1);
    pts.iter().step_by(step).copied().take(MAX_OUTLINE).collect()
}

// ---------------------------------------------------------------------------------------------
// small scanners

struct Scan<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Scan<'a> {
    fn new(s: &'a str) -> Scan<'a> {
        Scan { b: s.as_bytes(), i: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn skip_sep(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace() || c == b',') {
            self.i += 1;
        }
    }
    /// A finite number (SVG syntax: `-.5e-3`, `1.5.5` is two numbers).
    fn num(&mut self) -> Option<f64> {
        self.skip_sep();
        let s = self.i;
        let mut i = s;
        let at = |i: usize| self.b.get(i).copied();
        if matches!(at(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let d0 = i;
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        let mut digits = i - d0;
        if at(i) == Some(b'.') {
            i += 1;
            let f0 = i;
            while at(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            digits += i - f0;
        }
        if digits == 0 {
            return None;
        }
        if matches!(at(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(at(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if at(j).is_some_and(|c| c.is_ascii_digit()) {
                while at(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
                i = j;
            }
        }
        let v: f64 = std::str::from_utf8(self.b.get(s..i)?).ok()?.parse().ok()?;
        self.i = i;
        (v.is_finite() && v.abs() < 1e12).then_some(v)
    }
    /// An arc flag: a single `0` or `1`.
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = self.peek()?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// A length like `12.5mm` in px (96 dpi). Percentages are not supported.
fn length(s: &str) -> Option<f64> {
    let mut sc = Scan::new(s.trim());
    let v = sc.num()?;
    let unit = s.trim().get(sc.i..)?.trim();
    let k = match unit {
        "" | "px" => 1.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        "pt" => 96.0 / 72.0,
        "pc" => 16.0,
        "em" => 16.0,
        "ex" => 8.0,
        _ => return None,
    };
    Some(v * k)
}

/// Opacity is a number or percentage, not a CSS length.
fn opacity(s: &str) -> Option<f64> {
    let mut sc = Scan::new(s.trim());
    let v = sc.num()?;
    let tail = s.trim().get(sc.i..)?.trim();
    match tail {
        "" => Some(v.clamp(0.0, 1.0)),
        "%" => Some((v / 100.0).clamp(0.0, 1.0)),
        _ => None,
    }
}

fn numbers(s: &str, cap: usize) -> Vec<f64> {
    let mut sc = Scan::new(s);
    let mut v = Vec::new();
    while v.len() < cap {
        match sc.num() {
            Some(x) => v.push(x),
            None => break,
        }
    }
    v
}

fn parse_transform(s: &str) -> Affine {
    let mut cur = Affine::ID;
    let b = s.as_bytes();
    let mut i = 0;
    let mut count = 0;
    while i < b.len() && count < 32 {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        let st = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        let Some(name) = s.get(st..i) else { break };
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) != Some(&b'(') || name.is_empty() {
            break;
        }
        let a0 = i + 1;
        let Some(close) = s.get(a0..).and_then(|r| r.find(')')) else { break };
        let args = numbers(s.get(a0..a0 + close).unwrap_or(""), 6);
        i = a0 + close + 1;
        count += 1;
        let g = |k: usize, d: f64| args.get(k).copied().unwrap_or(d);
        let t = match name {
            "translate" => Affine([1.0, 0.0, 0.0, 1.0, g(0, 0.0), g(1, 0.0)]),
            "scale" => Affine([g(0, 1.0), 0.0, 0.0, g(1, g(0, 1.0)), 0.0, 0.0]),
            "rotate" => {
                let (s, c) = g(0, 0.0).to_radians().sin_cos();
                let r = Affine([c, s, -s, c, 0.0, 0.0]);
                let (cx, cy) = (g(1, 0.0), g(2, 0.0));
                Affine([1.0, 0.0, 0.0, 1.0, cx, cy]).mul(&r).mul(&Affine([1.0, 0.0, 0.0, 1.0, -cx, -cy]))
            }
            "matrix" if args.len() == 6 => Affine([args[0], args[1], args[2], args[3], args[4], args[5]]),
            "skewX" => Affine([1.0, 0.0, g(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0]),
            "skewY" => Affine([1.0, g(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => Affine::ID,
        };
        cur = cur.mul(&t);
    }
    cur
}

fn parse_color(s: &str) -> Option<[f32; 3]> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let v = |i: usize, w: usize| u32::from_str_radix(h.get(i..i + w)?, 16).ok();
        return match h.len() {
            3 => Some([v(0, 1)? as f32 * 17.0 / 255.0, v(1, 1)? as f32 * 17.0 / 255.0, v(2, 1)? as f32 * 17.0 / 255.0]),
            // #RGBA and #RRGGBBAA: the colour counts, the alpha is not used.
            4 => Some([v(0, 1)? as f32 * 17.0 / 255.0, v(1, 1)? as f32 * 17.0 / 255.0, v(2, 1)? as f32 * 17.0 / 255.0]),
            6 | 8 => Some([v(0, 2)? as f32 / 255.0, v(2, 2)? as f32 / 255.0, v(4, 2)? as f32 / 255.0]),
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    if let Some(r) = lower.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let mut sc = Scan::new(r);
        let mut rgb = [0.0; 3];
        for c in &mut rgb {
            let v = sc.num()?;
            let k = if sc.peek() == Some(b'%') {
                sc.i += 1;
                100.0
            } else {
                255.0
            };
            *c = (v / k).clamp(0.0, 1.0) as f32;
        }
        sc.skip_sep();
        return sc.peek().is_none().then_some(rgb);
    }
    // The basic CSS palette is frequent in hand-written SVGs and diagram exports.
    let rgb = match lower.as_str() {
        "black" => [0, 0, 0],
        "silver" => [192, 192, 192],
        "gray" | "grey" => [128, 128, 128],
        "white" => [255, 255, 255],
        "maroon" => [128, 0, 0],
        "red" => [255, 0, 0],
        "purple" => [128, 0, 128],
        "fuchsia" | "magenta" => [255, 0, 255],
        "green" => [0, 128, 0],
        "lime" => [0, 255, 0],
        "olive" => [128, 128, 0],
        "yellow" => [255, 255, 0],
        "navy" => [0, 0, 128],
        "blue" => [0, 0, 255],
        "teal" => [0, 128, 128],
        "aqua" | "cyan" => [0, 255, 255],
        "orange" => [255, 165, 0],
        "rebeccapurple" => [102, 51, 153],
        _ => return None,
    };
    Some(rgb.map(|v| v as f32 / 255.0))
}

// ---------------------------------------------------------------------------------------------
// XML

struct Tag<'a> {
    start: usize,
    name: String,
    attrs: Vec<(&'a str, &'a str)>,
    closing: bool,
    selfclose: bool,
}

impl<'a> Tag<'a> {
    /// The value of a property: the `style` attribute wins over the plain attribute.
    fn prop(&self, key: &str) -> Option<&str> {
        if let Some((_, st)) = self.attrs.iter().find(|(k, _)| *k == "style") {
            // As in CSS, the last declaration wins (exports often append overrides).
            for decl in st.split(';').rev() {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == key
                {
                    return Some(v.trim());
                }
            }
        }
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.trim())
    }
    fn attr(&self, key: &str) -> Option<&'a str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
    fn len(&self, key: &str) -> Option<f64> {
        self.attr(key).and_then(length)
    }
}

/// The next tag at or after `*pos` (comments, CDATA, `<?..?>` and DOCTYPE are skipped).
fn next_tag<'a>(src: &'a str, pos: &mut usize) -> Option<Tag<'a>> {
    let b = src.as_bytes();
    loop {
        let rest = src.get(*pos..)?;
        let lt = rest.find('<')?;
        let s = *pos + lt;
        let after = src.get(s..)?;
        if after.starts_with("<!--") {
            *pos = s + after.find("-->").map(|e| e + 3).unwrap_or(after.len());
            continue;
        }
        if after.starts_with("<![CDATA[") {
            *pos = s + after.find("]]>").map(|e| e + 3).unwrap_or(after.len());
            continue;
        }
        if after.starts_with("<?") {
            *pos = s + after.find("?>").map(|e| e + 2).unwrap_or(after.len());
            continue;
        }
        if after.starts_with("<!") {
            // DOCTYPE may hold [ ... ] with > inside
            let mut depth = 0i32;
            let mut j = s + 2;
            while j < b.len() {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'>' if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            *pos = (j + 1).min(b.len());
            continue;
        }
        let mut i = s + 1;
        let closing = b.get(i) == Some(&b'/');
        if closing {
            i += 1;
        }
        let n0 = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let raw = src.get(n0..i).unwrap_or("");
        let name = raw.rsplit(':').next().unwrap_or(raw).to_ascii_lowercase();
        let mut attrs = Vec::new();
        let mut selfclose = false;
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            match b.get(i) {
                None => {
                    *pos = b.len();
                    break;
                }
                Some(b'>') => {
                    *pos = i + 1;
                    break;
                }
                Some(b'/') => {
                    selfclose = true;
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let k0 = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
                i += 1;
            }
            if i == k0 {
                i += 1;
                continue;
            }
            let key = src.get(k0..i).unwrap_or("");
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut val = "";
            if b.get(i) == Some(&b'=') {
                i += 1;
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                match b.get(i) {
                    Some(&q) if q == b'"' || q == b'\'' => {
                        let v0 = i + 1;
                        let end = src.get(v0..).and_then(|r| r.find(q as char)).map(|e| v0 + e).unwrap_or(b.len());
                        val = src.get(v0..end).unwrap_or("");
                        i = (end + 1).min(b.len());
                    }
                    _ => {
                        let v0 = i;
                        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                            i += 1;
                        }
                        val = src.get(v0..i).unwrap_or("");
                    }
                }
            }
            if attrs.len() < 64 {
                attrs.push((key, val));
            }
        }
        if closing {
            selfclose = false;
        }
        return Some(Tag { start: s, name, attrs, closing, selfclose });
    }
}

// ---------------------------------------------------------------------------------------------
// flattening shapes

/// Counters shared by every element of one pass.
struct Budget {
    commands: usize,
    points: usize,
    elements: usize,
    visits: usize,
    /// Bytes of referenced fragments entered so far (`MAX_REFERENCE_BYTES`).
    reference_bytes: usize,
    limited: bool,
    /// A local reference pointed at a missing id or back into itself.
    unresolved: bool,
}

struct Raw {
    polys: Vec<Vec<(f64, f64)>>,
    evenodd: bool,
    color: Option<[f32; 3]>,
}

/// Builds polylines in document coordinates from local-space drawing calls.
struct Pen<'a> {
    xf: Affine,
    /// Allowed error in local units (None = rough pass).
    tol: Option<f64>,
    out: Vec<Vec<(f64, f64)>>,
    sub: Vec<(f64, f64)>,
    budget: &'a mut Budget,
}

impl Pen<'_> {
    fn pt(&mut self, x: f64, y: f64) -> Option<()> {
        self.budget.points += 1;
        if self.budget.points > MAX_POINTS {
            self.budget.limited = true;
            return None;
        }
        let (px, py) = self.xf.apply(x, y);
        if !px.is_finite() || !py.is_finite() || px.abs() > 1e12 || py.abs() > 1e12 {
            return None;
        }
        self.sub.push((px, py));
        Some(())
    }
    fn finish(&mut self) {
        let s = std::mem::take(&mut self.sub);
        if s.len() >= 3 {
            self.out.push(s);
        }
    }
    fn cubic(&mut self, p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> Option<()> {
        let n = match self.tol {
            None => 12,
            Some(t) => {
                let d1 = (p0.0 - 2.0 * p1.0 + p2.0).hypot(p0.1 - 2.0 * p1.1 + p2.1);
                let d2 = (p1.0 - 2.0 * p2.0 + p3.0).hypot(p1.1 - 2.0 * p2.1 + p3.1);
                ((0.75 * d1.max(d2) / t).sqrt().ceil() as usize).clamp(1, 500)
            }
        };
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.pt(a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)?;
        }
        Some(())
    }
    fn quad(&mut self, p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)) -> Option<()> {
        let n = match self.tol {
            None => 10,
            Some(t) => {
                let d = (p0.0 - 2.0 * p1.0 + p2.0).hypot(p0.1 - 2.0 * p1.1 + p2.1);
                ((d / (4.0 * t)).sqrt().ceil() as usize).clamp(1, 500)
            }
        };
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c) = (u * u, 2.0 * u * t, t * t);
            self.pt(a * p0.0 + b * p1.0 + c * p2.0, a * p0.1 + b * p1.1 + c * p2.1)?;
        }
        Some(())
    }
    /// Steps needed to sweep `sweep` radians on a circle of radius `r`.
    fn arc_steps(&self, r: f64, sweep: f64) -> usize {
        let step = match self.tol {
            None => std::f64::consts::PI / 16.0,
            Some(t) if t < r => 2.0 * (1.0 - t / r).acos(),
            Some(_) => std::f64::consts::FRAC_PI_2,
        };
        ((sweep.abs() / step.max(1e-6)).ceil() as usize).clamp(1, 720)
    }
    /// SVG endpoint arc from `p0` to `p1`.
    #[allow(clippy::too_many_arguments)]
    fn arc(&mut self, p0: (f64, f64), rx: f64, ry: f64, rot: f64, large: bool, sweep: bool, p1: (f64, f64)) -> Option<()> {
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if (p0.0 == p1.0 && p0.1 == p1.1) || rx < 1e-12 || ry < 1e-12 {
            return if p0 == p1 { Some(()) } else { self.pt(p1.0, p1.1) };
        }
        let (s, c) = rot.to_radians().sin_cos();
        let (dx, dy) = ((p0.0 - p1.0) * 0.5, (p0.1 - p1.1) * 0.5);
        let (x1, y1) = (c * dx + s * dy, -s * dx + c * dy);
        let lam = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
        if lam > 1.0 {
            let k = lam.sqrt();
            rx *= k;
            ry *= k;
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        if den < 1e-30 {
            return self.pt(p1.0, p1.1);
        }
        let k = (num / den).max(0.0).sqrt() * if large == sweep { -1.0 } else { 1.0 };
        let (cxp, cyp) = (k * rx * y1 / ry, -k * ry * x1 / rx);
        let (cx, cy) = (c * cxp - s * cyp + (p0.0 + p1.0) * 0.5, s * cxp + c * cyp + (p0.1 + p1.1) * 0.5);
        let t1 = ((y1 - cyp) / ry).atan2((x1 - cxp) / rx);
        let t2 = ((-y1 - cyp) / ry).atan2((-x1 - cxp) / rx);
        let mut dt = t2 - t1;
        if !sweep && dt > 0.0 {
            dt -= std::f64::consts::TAU;
        } else if sweep && dt < 0.0 {
            dt += std::f64::consts::TAU;
        }
        let n = self.arc_steps(rx.max(ry), dt);
        for i in 1..n {
            let th = t1 + dt * i as f64 / n as f64;
            let (st, ct) = th.sin_cos();
            self.pt(cx + rx * ct * c - ry * st * s, cy + rx * ct * s + ry * st * c)?;
        }
        self.pt(p1.0, p1.1)
    }
    /// An elliptical arc around (cx, cy) from angle `a0` to `a1`, both ends included.
    #[allow(clippy::too_many_arguments)]
    fn ellipse_arc(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64, with_start: bool) -> Option<()> {
        let n = self.arc_steps(rx.max(ry), a1 - a0);
        for i in (if with_start { 0 } else { 1 })..=n {
            let (s, c) = (a0 + (a1 - a0) * i as f64 / n as f64).sin_cos();
            self.pt(cx + rx * c, cy + ry * s)?;
        }
        Some(())
    }

    fn path(&mut self, d: &str) {
        let _ = self.run_path(d);
        self.finish();
    }

    fn run_path(&mut self, d: &str) -> Option<()> {
        let mut sc = Scan::new(d);
        let mut cmd = 0u8;
        let mut started = false;
        let (mut cx, mut cy) = (0.0_f64, 0.0_f64);
        let (mut sx, mut sy) = (0.0_f64, 0.0_f64);
        let mut last_c: Option<(f64, f64)> = None;
        let mut last_q: Option<(f64, f64)> = None;
        loop {
            sc.skip_sep();
            let c = sc.peek()?;
            if c.is_ascii_alphabetic() {
                if !b"MmLlHhVvCcSsQqTtAaZz".contains(&c) {
                    return None;
                }
                cmd = c;
                sc.i += 1;
                if cmd == b'Z' || cmd == b'z' {
                    if !started {
                        return None;
                    }
                    self.budget.commands += 1;
                    if self.budget.commands > MAX_COMMANDS {
                        self.budget.limited = true;
                        return None;
                    }
                    self.finish();
                    cx = sx;
                    cy = sy;
                    self.pt(cx, cy)?;
                    last_c = None;
                    last_q = None;
                    continue;
                }
            } else {
                match cmd {
                    0 | b'Z' | b'z' => return None,
                    b'M' => cmd = b'L',
                    b'm' => cmd = b'l',
                    _ => {}
                }
            }
            if !started && !matches!(cmd, b'M' | b'm') {
                return None;
            }
            self.budget.commands += 1;
            if self.budget.commands > MAX_COMMANDS {
                self.budget.limited = true;
                return None;
            }
            let rel = cmd.is_ascii_lowercase();
            let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
            match cmd.to_ascii_uppercase() {
                b'M' => {
                    let (x, y) = (sc.num()? + ox, sc.num()? + oy);
                    self.finish();
                    self.pt(x, y)?;
                    (cx, cy, sx, sy) = (x, y, x, y);
                    started = true;
                    last_c = None;
                    last_q = None;
                }
                b'L' => {
                    let (x, y) = (sc.num()? + ox, sc.num()? + oy);
                    self.pt(x, y)?;
                    (cx, cy) = (x, y);
                    last_c = None;
                    last_q = None;
                }
                b'H' => {
                    let x = sc.num()? + ox;
                    self.pt(x, cy)?;
                    cx = x;
                    last_c = None;
                    last_q = None;
                }
                b'V' => {
                    let y = sc.num()? + oy;
                    self.pt(cx, y)?;
                    cy = y;
                    last_c = None;
                    last_q = None;
                }
                b'C' | b'S' => {
                    let p1 = if cmd.eq_ignore_ascii_case(&b'C') {
                        (sc.num()? + ox, sc.num()? + oy)
                    } else {
                        last_c.map(|l| (2.0 * cx - l.0, 2.0 * cy - l.1)).unwrap_or((cx, cy))
                    };
                    let p2 = (sc.num()? + ox, sc.num()? + oy);
                    let p3 = (sc.num()? + ox, sc.num()? + oy);
                    self.cubic((cx, cy), p1, p2, p3)?;
                    (cx, cy) = p3;
                    last_c = Some(p2);
                    last_q = None;
                }
                b'Q' | b'T' => {
                    let p1 = if cmd.eq_ignore_ascii_case(&b'Q') {
                        (sc.num()? + ox, sc.num()? + oy)
                    } else {
                        last_q.map(|l| (2.0 * cx - l.0, 2.0 * cy - l.1)).unwrap_or((cx, cy))
                    };
                    let p2 = (sc.num()? + ox, sc.num()? + oy);
                    self.quad((cx, cy), p1, p2)?;
                    (cx, cy) = p2;
                    last_q = Some(p1);
                    last_c = None;
                }
                b'A' => {
                    let (rx, ry, rot) = (sc.num()?, sc.num()?, sc.num()?);
                    let (large, sweep) = (sc.flag()?, sc.flag()?);
                    let (x, y) = (sc.num()? + ox, sc.num()? + oy);
                    self.arc((cx, cy), rx, ry, rot, large, sweep, (x, y))?;
                    (cx, cy) = (x, y);
                    last_c = None;
                    last_q = None;
                }
                _ => return None,
            }
        }
    }

    fn polygon(&mut self, pts: &[f64]) {
        for p in pts.as_chunks::<2>().0.iter() {
            if self.pt(p[0], p[1]).is_none() {
                break;
            }
        }
        self.finish();
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: Option<f64>, ry: Option<f64>) {
        if !(w > 0.0 && h > 0.0) {
            return;
        }
        let (mut rx, mut ry) = match (rx, ry) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None) => (a, a),
            (None, Some(b)) => (b, b),
            (None, None) => (0.0, 0.0),
        };
        rx = rx.clamp(0.0, w * 0.5);
        ry = ry.clamp(0.0, h * 0.5);
        let _ = (|| -> Option<()> {
            if rx < 1e-9 || ry < 1e-9 {
                for (px, py) in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
                    self.pt(px, py)?;
                }
            } else {
                use std::f64::consts::{FRAC_PI_2, PI};
                // corners clockwise on screen, starting at the top-left corner's end of the top edge
                self.ellipse_arc(x + rx, y + ry, rx, ry, PI, PI + FRAC_PI_2, true)?;
                self.ellipse_arc(x + w - rx, y + ry, rx, ry, PI + FRAC_PI_2, 2.0 * PI, true)?;
                self.ellipse_arc(x + w - rx, y + h - ry, rx, ry, 0.0, FRAC_PI_2, true)?;
                self.ellipse_arc(x + rx, y + h - ry, rx, ry, FRAC_PI_2, PI, true)?;
            }
            Some(())
        })();
        self.finish();
    }

    fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        if !(rx > 0.0 && ry > 0.0) {
            return;
        }
        let n = self.arc_steps(rx.max(ry), std::f64::consts::TAU).max(8);
        for i in 0..n {
            let (s, c) = (std::f64::consts::TAU * i as f64 / n as f64).sin_cos();
            if self.pt(cx + rx * c, cy + ry * s).is_none() {
                break;
            }
        }
        self.finish();
    }
}

#[derive(Clone)]
struct State {
    xf: Affine,
    fill_none: bool,
    evenodd: bool,
    hidden: bool,
    fill_hidden: bool,
    color: Option<[f32; 3]>,
    /// The CSS `color` property, which `fill="currentColor"` means.
    current: Option<[f32; 3]>,
    current_fill: bool,
    /// The visible part of the nested `<svg>` viewports above this element, in document coordinates.
    clip: Option<Vec<(f64, f64)>>,
}

/// Names whose contents are never drawn by us.
fn skipped(name: &str) -> bool {
    matches!(
        name,
        "defs"
            | "clippath"
            | "mask"
            | "symbol"
            | "pattern"
            | "marker"
            | "title"
            | "desc"
            | "metadata"
            | "style"
            | "script"
            | "lineargradient"
            | "radialgradient"
            | "filter"
            | "foreignobject"
            | "text"
            | "image"
            | "switch"
            | "animate"
            | "animatetransform"
            | "set"
    )
}

/// Index id-bearing elements without executing scripts, loading URLs or expanding entities.
/// Referencing a subtree is bounded by both a recursion limit and the shared geometry budget.
fn collect_ids(src: &str) -> HashMap<&str, (usize, usize)> {
    let mut ids = HashMap::new();
    let mut open: Vec<(Option<&str>, usize)> = Vec::new();
    let mut pos = 0;
    while let Some(tag) = next_tag(src, &mut pos) {
        if tag.closing {
            if let Some((Some(id), begin)) = open.pop()
                && ids.len() < MAX_ELEMENTS
            {
                ids.entry(id).or_insert((begin, pos));
            }
            continue;
        }
        let id = tag.attr("id").filter(|v| !v.is_empty() && v.len() <= 128);
        if tag.selfclose {
            if let Some(id) = id
                && ids.len() < MAX_ELEMENTS
            {
                ids.entry(id).or_insert((tag.start, pos));
            }
        } else {
            if open.len() >= MAX_DEPTH {
                break;
            }
            open.push((id, tag.start));
        }
    }
    ids
}

/// Walk the document. Local #id references are expanded in the same paint order as <use>;
/// external hrefs are never read or fetched.
fn walk(src: &str, tol: Option<f64>, ids: &HashMap<&str, (usize, usize)>, limited: &mut bool, unresolved: &mut bool) -> Vec<Raw> {
    let mut budget = Budget { commands: 0, points: 0, elements: 0, visits: 0, reference_bytes: 0, limited: false, unresolved: false };
    let mut out = Vec::new();
    let initial = State {
        xf: Affine::ID,
        fill_none: false,
        evenodd: false,
        hidden: false,
        fill_hidden: false,
        color: None,
        current: None,
        current_fill: false,
        clip: None,
    };
    walk_part(src, src, tol, ids, &mut budget, &mut out, initial, None, &mut Vec::new());
    *limited |= budget.limited;
    *unresolved |= budget.unresolved;
    out
}

#[allow(clippy::too_many_arguments)]
fn walk_part<'a>(
    document: &'a str,
    fragment: &'a str,
    tol: Option<f64>,
    ids: &HashMap<&'a str, (usize, usize)>,
    budget: &mut Budget,
    out: &mut Vec<Raw>,
    inherited: State,
    reference_viewport: Option<[Option<f64>; 2]>,
    active: &mut Vec<String>,
) {
    if reference_viewport.is_some() {
        // Charge the whole referenced fragment before entering it. This also bounds the work of
        // scanning repeated comments or style text, not just the geometry it emits.
        if fragment.len() > MAX_REFERENCE_BYTES.saturating_sub(budget.reference_bytes) {
            budget.limited = true;
            return;
        }
        budget.reference_bytes += fragment.len();
    }
    let mut stack = vec![inherited];
    let mut skip = 0usize;
    let mut over = 0usize;
    let mut first = true;
    let mut pos = 0usize;
    while let Some(tag) = next_tag(fragment, &mut pos) {
        if budget.visits >= MAX_VISITS || budget.elements >= MAX_ELEMENTS || budget.points >= MAX_POINTS || budget.commands >= MAX_COMMANDS {
            budget.limited = true;
            break;
        }
        budget.visits += 1;
        if tag.closing {
            if skip > 0 {
                skip -= 1;
            } else if over > 0 {
                over -= 1;
            } else if stack.len() > 1 {
                stack.pop();
            }
            continue;
        }
        if skip > 0 || over > 0 {
            if !tag.selfclose {
                if skip > 0 {
                    skip += 1;
                } else {
                    over += 1;
                }
            }
            continue;
        }
        let name = tag.name.as_str();
        let reference_root = first && reference_viewport.is_some();
        let outermost = first && reference_viewport.is_none();
        first = false;
        let hide = tag.prop("display").is_some_and(|v| v == "none") || tag.prop("opacity").and_then(opacity).is_some_and(|o| o <= 0.0);
        if (skipped(name) && !(reference_root && name == "symbol")) || hide {
            if !tag.selfclose {
                skip = 1;
                if name == "style" || name == "script" {
                    let close = format!("</{name}");
                    if let Some(i) = fragment.get(pos..).and_then(|r| r.find(&close)) {
                        pos += i;
                    }
                }
            }
            continue;
        }
        let Some(parent) = stack.last().cloned() else { break };
        let mut st = parent;
        let mut xf = st.xf;
        if let Some(t) = tag.attr("transform") {
            xf = xf.mul(&parse_transform(t));
        }
        if name == "svg" || (reference_root && name == "symbol") {
            if name == "svg" {
                xf = xf.mul(&Affine([1.0, 0.0, 0.0, 1.0, tag.len("x").unwrap_or(0.0), tag.len("y").unwrap_or(0.0)]));
            }
            // A nested viewport (or a symbol/template shown by <use>) clips what it shows. The
            // outermost <svg> is the picture itself, so it has no viewport above it to clip to.
            if !outermost && clips_viewport(&tag) {
                let [uw, uh] = reference_viewport.unwrap_or([None, None]);
                let w = if reference_root { uw.or_else(|| tag.len("width")) } else { tag.len("width") };
                let h = if reference_root { uh.or_else(|| tag.len("height")) } else { tag.len("height") };
                if let (Some(w), Some(h)) = (w, h)
                    && let Some(rect) = view_rect(&xf, w, h)
                {
                    st.clip = Some(match st.clip.take() {
                        Some(old) => clip_polygon(&old, &rect),
                        None => rect,
                    });
                }
            }
            xf = xf.mul(&root_transform(&tag, if reference_root { reference_viewport } else { None }));
        }
        st.xf = xf;
        match tag.prop("visibility") {
            Some("hidden") | Some("collapse") => st.hidden = true,
            Some("visible") => st.hidden = false,
            _ => {}
        }
        if let Some(r) = tag.prop("fill-rule") {
            match r {
                "evenodd" => st.evenodd = true,
                "nonzero" => st.evenodd = false,
                _ => {}
            }
        }
        if let Some(o) = tag.prop("fill-opacity").and_then(opacity) {
            st.fill_hidden = o <= 0.0;
        }
        if let Some(c) = tag.prop("color").and_then(parse_color) {
            st.current = Some(c);
            // A fill of currentColor follows the colour even when it is set on a child.
            if st.current_fill {
                st.color = Some(c);
            }
        }
        if let Some(f) = tag.prop("fill") {
            st.current_fill = f.trim().eq_ignore_ascii_case("currentcolor");
            match f {
                "inherit" | "unset" => {}
                "none" | "transparent" => st.fill_none = true,
                _ if st.current_fill => {
                    st.color = st.current;
                    st.fill_none = false;
                }
                _ => {
                    st.color = parse_color(f).or(st.color);
                    st.fill_none = false;
                }
            }
        }
        if name == "use" {
            if !st.hidden
                && active.len() < 16
                && let Some(id) = tag.attr("href").or_else(|| tag.attr("xlink:href")).and_then(|v| v.strip_prefix('#'))
            {
                if active.iter().any(|v| v == id) {
                    budget.unresolved = true;
                } else if let Some(&(begin, end)) = ids.get(id)
                    && let Some(part) = document.get(begin..end)
                {
                    // x/y is a translation inside the use element's own transform.
                    st.xf = st.xf.mul(&Affine([1.0, 0.0, 0.0, 1.0, tag.len("x").unwrap_or(0.0), tag.len("y").unwrap_or(0.0)]));
                    if st.xf.ok() {
                        active.push(id.to_owned());
                        walk_part(document, part, tol, ids, budget, out, st, Some([tag.len("width"), tag.len("height")]), active);
                        active.pop();
                    }
                } else {
                    budget.unresolved = true;
                }
            }
            if active.len() >= 16 {
                budget.limited = true;
            }
            if !tag.selfclose {
                skip = 1;
            }
            continue;
        }
        let drawable = matches!(name, "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline");
        if drawable {
            if !st.hidden && !st.fill_none && !st.fill_hidden && xf.ok() {
                budget.elements += 1;
                let sf = xf.scale().max(1e-12);
                let mut pen = Pen { xf, tol: tol.map(|t| t / sf), out: Vec::new(), sub: Vec::new(), budget };
                let n = |k: &str| tag.len(k);
                match name {
                    "path" => {
                        if let Some(d) = tag.attr("d") {
                            pen.path(d);
                        }
                    }
                    "polygon" | "polyline" => {
                        let v = numbers(tag.attr("points").unwrap_or(""), 100_000);
                        pen.polygon(&v);
                    }
                    "rect" => pen.rect(
                        n("x").unwrap_or(0.0),
                        n("y").unwrap_or(0.0),
                        n("width").unwrap_or(0.0),
                        n("height").unwrap_or(0.0),
                        n("rx"),
                        n("ry"),
                    ),
                    "circle" => pen.ellipse(n("cx").unwrap_or(0.0), n("cy").unwrap_or(0.0), n("r").unwrap_or(0.0), n("r").unwrap_or(0.0)),
                    _ => {
                        let (rx, ry) = (n("rx"), n("ry"));
                        pen.ellipse(n("cx").unwrap_or(0.0), n("cy").unwrap_or(0.0), rx.or(ry).unwrap_or(0.0), ry.or(rx).unwrap_or(0.0));
                    }
                }
                // Do not retain only the beginning of a compound path: later subpaths may be holes.
                let polys = if pen.budget.limited {
                    Vec::new()
                } else {
                    let drawn = std::mem::take(&mut pen.out);
                    match st.clip.as_deref() {
                        None => drawn,
                        Some(clip) => drawn
                            .into_iter()
                            .filter_map(|p| {
                                let inside = clip_polygon(&p, clip);
                                (inside.len() >= 3).then_some(inside)
                            })
                            .collect(),
                    }
                };
                if !polys.is_empty() {
                    out.push(Raw { polys, evenodd: st.evenodd, color: st.color });
                }
            }
            if !tag.selfclose {
                skip = 1;
            }
            continue;
        }
        if !tag.selfclose {
            if stack.len() >= MAX_DEPTH {
                budget.limited = true;
                over = 1;
            } else {
                stack.push(st);
            }
        }
    }
}

/// The `<svg>` viewport rectangle (width × height in its own coordinates) in document space.
fn view_rect(xf: &Affine, w: f64, h: f64) -> Option<Vec<(f64, f64)>> {
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return None;
    }
    let pts: Vec<(f64, f64)> = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].iter().map(|&(x, y)| xf.apply(x, y)).collect();
    pts.iter().all(|p| p.0.is_finite() && p.1.is_finite() && p.0.abs() < 1e12 && p.1.abs() < 1e12).then_some(pts)
}

/// A nested viewport clips by default; `overflow="visible"` shows the geometry outside it.
fn clips_viewport(tag: &Tag) -> bool {
    !tag.prop("overflow").is_some_and(|v| v.trim().eq_ignore_ascii_case("visible"))
}

/// The matrix from the `<svg>` element's viewBox to its width and height (px).
fn root_transform(tag: &Tag, viewport: Option<[Option<f64>; 2]>) -> Affine {
    let vb = tag.attr("viewBox").map(|v| numbers(v, 4)).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    let [uw, uh] = viewport.unwrap_or([None, None]);
    let (w, h) = (uw.or_else(|| tag.len("width")), uh.or_else(|| tag.len("height")));
    let Some(vb) = vb else { return Affine::ID };
    let (vw, vh) = match (w, h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * vb[3] / vb[2]),
        (None, Some(h)) => (h * vb[2] / vb[3], h),
        (None, None) => (vb[2], vb[3]),
    };
    if !(vw > 0.0 && vh > 0.0) {
        return Affine::ID;
    }
    let (sx, sy) = (vw / vb[2], vh / vb[3]);
    let mut aspect = tag.attr("preserveAspectRatio").unwrap_or("xMidYMid meet").split_ascii_whitespace();
    let align = match aspect.next() {
        Some("defer") => aspect.next().unwrap_or("xMidYMid"),
        Some(a) => a,
        None => "xMidYMid",
    };
    if align == "none" {
        return Affine([sx, 0.0, 0.0, sy, -vb[0] * sx, -vb[1] * sy]);
    }
    let s = if aspect.next() == Some("slice") { sx.max(sy) } else { sx.min(sy) };
    let ax = if align.starts_with("xMin") {
        0.0
    } else if align.starts_with("xMax") {
        1.0
    } else {
        0.5
    };
    let ay = if align.ends_with("YMin") {
        0.0
    } else if align.ends_with("YMax") {
        1.0
    } else {
        0.5
    };
    Affine([s, 0.0, 0.0, s, (vw - vb[2] * s) * ax - vb[0] * s, (vh - vb[3] * s) * ay - vb[1] * s])
}

// ---------------------------------------------------------------------------------------------
// outlines -> solids and holes

struct Cand {
    pts: Vec<(f64, f64)>,
    area: f64,
    point: (f64, f64),
    element: usize,
    color: Option<[f32; 3]>,
}

fn build_artwork(raws: &[Raw], side: f64) -> Result<Artwork, String> {
    let (lo, hi) = bounds(raws.iter().flat_map(|r| r.polys.iter().flatten())).ok_or("No shape found")?;
    let centre = ((lo.0 + hi.0) * 0.5, (lo.1 + hi.1) * 0.5);
    let mut dropped = 0usize;
    let mut cands: Vec<Cand> = Vec::new();
    for (ei, r) in raws.iter().enumerate() {
        let element_start = cands.len();
        let mut over_limit = false;
        for poly in &r.polys {
            // to Y up, centred
            let p: Vec<(f64, f64)> = poly.iter().map(|q| (q.0 - centre.0, -(q.1 - centre.1))).collect();
            let s = simplify(&p, side);
            let a = area(&s);
            if s.len() < 3 || a.abs() < side * side * 1e-6 {
                dropped += 1;
                continue;
            }
            if cands.len() >= MAX_CONTOURS {
                over_limit = true;
                dropped += 1;
                continue;
            }
            let point = interior_point(&s);
            cands.push(Cand { pts: s, area: a, point, element: ei, color: r.color });
        }
        if over_limit {
            // Dropping just the last contours can erase a compound path's holes.
            dropped += cands.len() - element_start;
            cands.truncate(element_start);
        }
    }
    if cands.is_empty() {
        return Err("No filled shape found (line-only drawings can't be read)".into());
    }
    // containers of each outline (only inside the same shape element)
    let n = cands.len();
    let mut containers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..n {
            if i != j && cands[i].element == cands[j].element && cands[j].area.abs() > cands[i].area.abs() && inside(&cands[j].pts, cands[i].point) {
                containers[i].push(j);
            }
        }
    }
    // solid / hole / nothing
    let mut role: Vec<Option<bool>> = vec![None; n]; // Some(true) = hole
    for i in 0..n {
        if containers[i].len() > MAX_NEST {
            dropped += 1;
            continue;
        }
        let evenodd = raws.get(cands[i].element).is_some_and(|r| r.evenodd);
        if evenodd {
            role[i] = Some(containers[i].len() % 2 == 1);
        } else {
            let w: i32 = containers[i].iter().map(|&j| if cands[j].area > 0.0 { 1 } else { -1 }).sum();
            let si = if cands[i].area > 0.0 { 1 } else { -1 };
            match (w != 0, w + si != 0) {
                (false, true) => role[i] = Some(false),
                (true, false) => role[i] = Some(true),
                _ => dropped += 1,
            }
        }
    }
    // parents (the smallest container), then solid-then-its-holes order
    let parent: Vec<Option<usize>> = (0..n)
        .map(|i| {
            containers[i].iter().copied().min_by(|&a, &b| cands[a].area.abs().partial_cmp(&cands[b].area.abs()).unwrap_or(std::cmp::Ordering::Equal))
        })
        .collect();
    let mut order = Vec::new();
    let mut stack: Vec<usize> = (0..n).rev().filter(|&i| parent[i].is_none()).collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        for k in (0..n).rev() {
            if parent[k] == Some(i) {
                stack.push(k);
            }
        }
    }
    let mut contours: Vec<Contour> = Vec::new();
    for i in order {
        let Some(hole) = role[i] else { continue };
        let mut pts: Vec<[f32; 2]> = cands[i].pts.iter().map(|q| [q.0 as f32, q.1 as f32]).collect();
        if area32(&pts) < 0.0 {
            pts.reverse();
        }
        contours.push(Contour { pts, hole, color: cands[i].color });
    }
    if !contours.iter().any(|c| !c.hole) {
        return Err("No filled shape found".into());
    }
    // longest side = 1, centred on what is left
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in contours.iter().flat_map(|c| &c.pts) {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let w = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if w.is_nan() || w <= 0.0 || !w.is_finite() {
        return Err("Shape is too small to read".into());
    }
    let mid = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    for p in contours.iter_mut().flat_map(|c| c.pts.iter_mut()) {
        *p = [(p[0] - mid[0]) / w, (p[1] - mid[1]) / w];
    }
    let px_to_mm = 25.4 / 96.0;
    Ok(Artwork {
        contours,
        aspect: [(hi[0] - lo[0]) / w, (hi[1] - lo[1]) / w],
        natural_mm: [((hi[0] - lo[0]) as f64 * px_to_mm) as f32, ((hi[1] - lo[1]) as f64 * px_to_mm) as f32],
        dropped,
        warnings: Vec::new(),
    })
}

// ---------------------------------------------------------------------------------------------
// the scene

pub struct BuildOpts {
    /// The longest side of the picture, mm.
    pub target_mm: f32,
    /// How tall the extrusion is, mm.
    pub height_mm: f32,
    /// Rounding of the edges, mm.
    pub round_mm: f32,
    /// Upside down.
    pub flip: bool,
    /// Where the picture's centre goes (X, Y), mm.
    pub at: [f32; 2],
}

impl Default for BuildOpts {
    fn default() -> BuildOpts {
        BuildOpts { target_mm: 60.0, height_mm: 10.0, round_mm: 0.0, flip: false, at: [0.0, 0.0] }
    }
}

/// The import group `name` and its child groups. An `Extrude` is made for every solid and a taller
/// minus `Extrude` for every hole. A solid with holes sits in a child group together with those
/// holes, melting into the import group; a hole therefore only cuts the solid it belongs to.
/// At most `room` shapes and `group_room` groups (the import group is the first one); a solid and
/// its holes always stay together. The second value says when complete components were left out.
pub fn build_group(art: &Artwork, name: &str, o: &BuildOpts, room: usize, group_room: usize) -> (Vec<Group>, Option<String>) {
    let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
    let h = fin(o.height_mm, 10.0, 0.2, 500.0);
    let round = fin(o.round_mm, 0.0, 0.0, 100.0).min(h * 0.5);
    let at = [fin(o.at[0], 0.0, -1000.0, 1000.0), fin(o.at[1], 0.0, -1000.0, 1000.0)];
    if group_room == 0 {
        return (Vec::new(), Some(format!("{} SVG outlines did not fit (no group room). Solids and their holes are kept together", art.solids())));
    }
    let mut parent = Group::new(name);
    let mut children: Vec<Group> = Vec::new();
    let child_room = group_room.saturating_sub(1);
    let mut left_out = 0usize;
    let (mut ns, mut nh) = (0usize, 0usize);
    let mut used = 0usize;
    let mut part = 0usize;
    let mut contours = art.placed(o.target_mm, o.flip).into_iter();
    while let Some(first) = contours.next() {
        let holes = contours.as_slice().iter().take_while(|c| c.hole).count();
        let count = 1 + holes;
        let full = count > room.saturating_sub(used);
        if first.hole || full || (holes > 0 && children.len() >= child_room) {
            left_out += count;
            for _ in 0..holes {
                contours.next();
            }
            continue;
        }
        let mut shapes = Vec::with_capacity(count);
        for c in std::iter::once(first).chain(contours.by_ref().take(holes)) {
            let mut sh = Shape::new(Kind::Extrude, [at[0], at[1], h * 0.5], Material::plain(c.color.unwrap_or([0.93, 0.56, 0.45]), 0.45, 0.0));
            sh.outline = c.pts;
            sh.round = round;
            sh.blend = 0.0;
            if c.hole {
                nh += 1;
                sh.name = format!("{name} hole {nh}");
                sh.op = Op::Subtract;
                sh.size[0] = h + 2.0;
                sh.material = Material::plain([0.9, 0.35, 0.4], 0.6, 0.0);
            } else {
                ns += 1;
                sh.name = format!("{name} {ns}");
                sh.size[0] = h;
            }
            shapes.push(sh);
        }
        used += count;
        if holes == 0 {
            // No holes, so nothing can leak out of it: it can stay in the import group.
            parent.shapes.append(&mut shapes);
        } else {
            part += 1;
            let mut child = Group::new(format!("{name} {part}"));
            child.depth = 1;
            child.shapes = shapes;
            children.push(child);
        }
    }
    let mut groups = Vec::with_capacity(1 + children.len());
    groups.push(parent);
    groups.append(&mut children);
    let note = (left_out > 0).then(|| format!("{left_out} SVG outlines did not fit. Solids and their holes are kept together"));
    (groups, note)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"#)
    }
    /// Size of the picture in mm for a 60 mm target.
    fn extent(art: &Artwork) -> ([f32; 2], [f32; 2]) {
        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for c in art.placed(60.0, false) {
            for p in c.pts {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }
    fn solid_area(art: &Artwork) -> f32 {
        art.placed(60.0, false).iter().map(|c| if c.hole { -area32(&c.pts) } else { area32(&c.pts) }).sum()
    }
    fn all_shapes(groups: &[Group]) -> Vec<&Shape> {
        groups.iter().flat_map(|g| &g.shapes).collect()
    }
    fn all_ops(groups: &[Group]) -> Vec<Op> {
        groups.iter().flat_map(|g| g.shapes.iter().map(|s| s.op)).collect()
    }

    #[test]
    fn a_rect_becomes_one_ccw_solid_scaled_to_the_target() {
        let a = parse(&wrap(r#"<rect x="10" y="20" width="40" height="20"/>"#)).unwrap();
        assert_eq!((a.solids(), a.holes()), (1, 0));
        let (lo, hi) = extent(&a);
        assert!((hi[0] - lo[0] - 60.0).abs() < 1e-3 && (hi[1] - lo[1] - 30.0).abs() < 1e-3);
        assert!((lo[0] + hi[0]).abs() < 1e-3 && (lo[1] + hi[1]).abs() < 1e-3, "centred");
        assert!(area32(&a.placed(60.0, false)[0].pts) > 0.0);
        assert!(a.placed(60.0, true)[0].pts.len() == 4 && area32(&a.placed(60.0, true)[0].pts) > 0.0);
    }

    #[test]
    fn svg_y_goes_down_so_it_is_flipped() {
        // a triangle with its point at the top of the picture (small y)
        let a = parse(&wrap(r#"<polygon points="50,0 100,100 0,100"/>"#)).unwrap();
        let top = a.placed(60.0, false)[0].pts.iter().fold(f32::MIN, |m, p| m.max(p[1]));
        let apex = a.placed(60.0, false)[0].pts.iter().find(|p| p[1] == top).map(|p| p[0]).unwrap();
        assert!(apex.abs() < 1e-3, "the apex is at the top (+Y)");
        let apex2 = a.placed(60.0, true)[0].pts.iter().fold(f32::MIN, |m, p| m.max(p[1]));
        assert!(apex2 > 0.0);
        let low = a.placed(60.0, true)[0].pts.iter().find(|p| p[1] == apex2).map(|p| p[0]).unwrap();
        assert!(low.abs() > 1.0, "upside down: the wide side is at the top");
    }

    #[test]
    fn rounded_rect_circle_and_ellipse() {
        let a = parse(&wrap(r#"<rect width="80" height="40" rx="10"/><circle cx="50" cy="50" r="10"/><ellipse cx="50" cy="50" rx="20" ry="5"/>"#))
            .unwrap();
        assert_eq!(a.solids(), 3);
        let c = parse(&wrap(r#"<circle cx="50" cy="50" r="30"/>"#)).unwrap();
        assert!(c.contours[0].pts.len() <= MAX_OUTLINE && c.contours[0].pts.len() >= 16);
        let rr = parse(&wrap(r#"<rect width="80" height="40" rx="10"/>"#)).unwrap();
        // rounded: a corner of the box is cut
        let corner = rr.placed(60.0, false)[0].pts.iter().any(|p| (p[0] + 30.0).abs() < 0.2 && (p[1] - 15.0).abs() < 0.2);
        assert!(!corner);
    }

    #[test]
    fn simplified_circle_area_is_within_one_percent() {
        let a = parse(&wrap(r#"<circle cx="50" cy="50" r="40"/>"#)).unwrap();
        let r = 30.0_f32; // 60 mm across
        let want = std::f32::consts::PI * r * r;
        let got = solid_area(&a);
        assert!(((got - want) / want).abs() < 0.01, "{got} vs {want}");
        // the same circle drawn with arcs
        let b = parse(&wrap(r#"<path d="M10 50 a40 40 0 1 0 80 0 a40 40 0 1 0 -80 0 Z"/>"#)).unwrap();
        let got = solid_area(&b);
        assert!(((got - want) / want).abs() < 0.01, "{got} vs {want}");
        assert_eq!(b.solids(), 1);
    }

    #[test]
    fn paths_with_every_command_work() {
        let d =
            "M10 10 h30 v10 H10 z M60,60 l10,0 l0,10 l-10,0 Z M10 80 C 10 60, 30 60, 30 80 S 50 100, 50 80 Q 60 60 70 80 T 90 80 L 90 95 L 10 95 Z";
        let a = parse(&wrap(&format!(r#"<path d="{d}"/>"#))).unwrap();
        assert_eq!(a.solids(), 3);
        // relative curves land where absolute ones would
        let abs = parse(&wrap(r#"<path d="M10 50 C 20 10 40 10 50 50 L 10 90 Z"/>"#)).unwrap();
        let rel = parse(&wrap(r#"<path d="m10 50 c 10 -40 30 -40 40 0 l -40 40 z"/>"#)).unwrap();
        assert_eq!(abs.contours.len(), rel.contours.len());
        for (p, q) in abs.contours[0].pts.iter().zip(&rel.contours[0].pts) {
            assert!((p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5);
        }
        // cubic bulges upward as drawn (SVG y 10..50 -> a point above the baseline)
        let top = abs.placed(60.0, false)[0].pts.iter().fold(f32::MIN, |m, p| m.max(p[1]));
        assert!(top > 10.0);
    }

    #[test]
    fn an_arc_with_flags_packed_together_and_sweep_directions() {
        // flags written without spaces: "a20 20 0 01 40 0"
        let up = parse(&wrap(r#"<path d="M30 50 a20 20 0 0140 0 Z"/>"#)).unwrap();
        let down = parse(&wrap(r#"<path d="M30 50 a20 20 0 0040 0 Z"/>"#)).unwrap();
        let hy = |a: &Artwork| a.placed(60.0, false)[0].pts.iter().fold(0.0_f32, |m, p| m + p[1]) / a.contours[0].pts.len() as f32;
        // sweep=1 goes clockwise on screen: through the top, which is +Y once flipped
        assert!(hy(&up) > hy(&down));
        // half disc of radius 20 (the picture is 40 wide => 60 mm): area = pi r^2 / 2 with r = 30
        let want = std::f32::consts::PI * 900.0 / 2.0;
        assert!(((solid_area(&up) - want) / want).abs() < 0.02, "{}", solid_area(&up));
    }

    #[test]
    fn transforms_and_nested_groups() {
        let a = parse(&wrap(r#"<g transform="translate(50 50) scale(2)"><rect x="-10" y="-5" width="20" height="10" transform="rotate(90)"/></g>"#))
            .unwrap();
        // 20x10 rotated 90 and doubled: 20 wide x 40 tall
        let (lo, hi) = extent(&a);
        let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
        assert!((h - 60.0).abs() < 1e-2 && (w - 30.0).abs() < 1e-2, "{w} {h}");
        let m = parse(&wrap(r#"<rect width="10" height="10" transform="matrix(2 0 0 1 5 5)"/><rect x="40" width="10" height="10"/>"#)).unwrap();
        assert_eq!(m.solids(), 2);
        // the transform list applies left to right as nested transforms
        let t1 = parse_transform("translate(10 0) scale(2)").apply(1.0, 1.0);
        assert_eq!(t1, (12.0, 2.0));
        let r = parse_transform("rotate(90 10 10)").apply(20.0, 10.0);
        assert!((r.0 - 10.0).abs() < 1e-9 && (r.1 - 20.0).abs() < 1e-9);
    }

    #[test]
    fn view_box_and_units_scale_the_natural_size() {
        let mm = parse(r#"<svg viewBox="0 0 50 25" width="100mm" height="50mm"><rect width="50" height="25"/></svg>"#).unwrap();
        assert!((mm.natural_mm[0] - 100.0).abs() < 0.05 && (mm.natural_mm[1] - 50.0).abs() < 0.05, "{:?}", mm.natural_mm);
        let inch = parse(r#"<svg width="2in" height="1in"><rect width="192" height="96"/></svg>"#).unwrap();
        assert!((inch.natural_mm[0] - 50.8).abs() < 0.05);
        let pt = parse(r#"<svg viewBox="0 0 10 10" width="72pt" height="72pt"><rect width="10" height="10"/></svg>"#).unwrap();
        assert!((pt.natural_mm[0] - 25.4).abs() < 0.05);
        assert!((length("2.54cm").unwrap() - 96.0).abs() < 1e-9 && (length("10mm").unwrap() - 37.795).abs() < 1e-2);
        assert!(length("50%").is_none());
        // a viewBox with an offset
        let off = parse(r#"<svg viewBox="100 100 50 50"><rect x="100" y="100" width="25" height="50"/></svg>"#).unwrap();
        assert!((off.aspect[0] - 0.5).abs() < 1e-4 && (off.aspect[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn even_odd_makes_holes_and_islands() {
        let ring = parse(&wrap(r#"<path fill-rule="evenodd" d="M10 10 H90 V90 H10 Z M30 30 H70 V70 H30 Z M45 45 H55 V55 H45 Z"/>"#)).unwrap();
        // outer solid, hole, island in the hole
        let roles: Vec<bool> = ring.contours.iter().map(|c| c.hole).collect();
        assert_eq!(roles, vec![false, true, false]);
        // nonzero with opposite direction is a hole too; same direction fills it
        let opp = parse(&wrap(r#"<path d="M10 10 H90 V90 H10 Z M30 30 V70 H70 V30 Z"/>"#)).unwrap();
        assert_eq!((opp.solids(), opp.holes()), (1, 1));
        let same = parse(&wrap(r#"<path d="M10 10 H90 V90 H10 Z M30 30 H70 V70 H30 Z"/>"#)).unwrap();
        assert_eq!((same.solids(), same.holes()), (1, 0));
        // two separate shapes that overlap are not holes of each other
        let two = parse(&wrap(r#"<rect width="60" height="60"/><rect x="10" y="10" width="20" height="20"/>"#)).unwrap();
        assert_eq!((two.solids(), two.holes()), (2, 0));
    }

    #[test]
    fn a_hole_follows_its_solid_in_the_group() {
        let a = parse(SAMPLE_SVG).unwrap();
        assert_eq!((a.solids(), a.holes()), (2, 1));
        let (groups, note) = build_group(&a, "sample", &BuildOpts { height_mm: 8.0, ..BuildOpts::default() }, 64, 8);
        assert!(note.is_none());
        let shapes = all_shapes(&groups);
        assert_eq!(shapes.len(), 3);
        assert_eq!(groups[0].depth, 0, "the import group comes first");
        assert!(groups[1..].iter().all(|g| g.depth == 1), "each solid with holes melts in from a child group");
        let ring = groups.iter().find(|g| g.shapes.iter().any(|s| s.op == Op::Subtract)).expect("the ring group");
        let ops: Vec<Op> = ring.shapes.iter().map(|s| s.op).collect();
        let hole = ops.iter().position(|o| *o == Op::Subtract).unwrap();
        assert!(hole > 0 && ops[hole - 1] == Op::Add);
        assert!(shapes.iter().all(|s| s.kind == Kind::Extrude && s.blend == 0.0 && s.outline.len() >= 3 && s.outline.len() <= MAX_OUTLINE));
        assert!(ring.shapes[hole].size[0] > ring.shapes[hole - 1].size[0]);
        // the star keeps its 10 corners
        assert!(a.contours.iter().any(|c| !c.hole && c.pts.len() == 10));
        // limits
        let (g2, note2) = build_group(&a, "sample", &BuildOpts::default(), 2, 8);
        assert_eq!(all_shapes(&g2).len(), 1);
        assert!(note2.is_some(), "the ring is omitted together with its hole");
        // a scene with it stays valid, and the whole import is one block of groups
        let mut s = crate::model::Scene::default();
        s.groups.clear();
        s.groups.extend(groups);
        let back = crate::model::Scene::from_json(&s.to_json()).unwrap();
        assert_eq!(back.groups.iter().map(|g| g.shapes.len()).sum::<usize>(), 3);
    }

    #[test]
    fn a_hole_of_one_element_does_not_cut_another_elements_fill() {
        let a = parse(&wrap(
            r##"<path fill="#ff8c00" fill-rule="evenodd"
                  d="M0 0 H100 V100 H0 Z M30 30 H70 V70 H30 Z"/>
                <rect x="42" y="42" width="16" height="16" fill="#0088ee"/>"##,
        ))
        .unwrap();
        assert_eq!((a.solids(), a.holes()), (2, 1));
        let (groups, note) = build_group(&a, "overlap", &BuildOpts::default(), 64, 8);
        assert!(note.is_none());
        // The orange solid and its hole share a child group; the blue rect is added to the import group.
        assert_eq!(groups[0].shapes.len(), 1);
        let ring = groups.iter().find(|g| g.shapes.iter().any(|s| s.op == Op::Subtract)).expect("the orange hole group");
        assert_eq!(ring.shapes.iter().map(|s| s.op).collect::<Vec<_>>(), vec![Op::Add, Op::Subtract]);
        // The picture is 60 mm wide and centred: SVG (50, 50) is the world origin, (35, 35) is in the hole.
        let mut scene = crate::model::Scene::default();
        scene.groups.clear();
        scene.groups = groups;
        let field = crate::sdf::Field::new(&scene);
        assert!(field.distance([0.0, 0.0, 5.0]) < 0.0, "the blue rectangle inside the orange hole stays solid");
        assert!(field.distance([0.0, 0.0, 5.0]) <= -4.0, "it is solid well inside the rectangle, not a thin sliver");
        assert!(field.distance([9.0, 9.0, 5.0]) > 0.0, "the orange hole is still open around the rectangle");
        assert!(field.distance([-15.0, 15.0, 5.0]) < 0.0, "the orange solid around the hole is still there");
    }

    #[test]
    fn at_most_64_points_whatever_the_curve() {
        // a wiggly path with thousands of points
        let mut d = String::from("M50 50");
        for i in 0..3000 {
            let t = i as f64 * 0.01;
            d.push_str(&format!(" L{:.3} {:.3}", 50.0 + (10.0 + t) * t.cos(), 50.0 + (10.0 + t) * t.sin()));
        }
        d.push('Z');
        let a = parse(&wrap(&format!(r#"<path d="{d}"/>"#)));
        let a = a.unwrap();
        assert!(a.contours.iter().all(|c| c.pts.len() <= MAX_OUTLINE && c.pts.len() >= 3));
        let rdp = simplify(&(0..500).map(|i| ((i as f64 * 0.1).cos() * 50.0, (i as f64 * 0.1).sin() * 50.0)).collect::<Vec<_>>(), 100.0);
        assert!(rdp.len() <= MAX_OUTLINE);
    }

    #[test]
    fn hidden_things_and_junk_are_skipped() {
        let a = parse(&wrap(
            r##"<defs><rect width="50" height="50"/></defs><rect style="display:none" width="50" height="50"/>
               <g display="none"><rect width="9" height="9"/></g><rect width="10" height="10" fill="none" stroke="red"/>
               <line x1="0" y1="0" x2="50" y2="50"/><use href="#a"/><!-- <rect width="1" height="1"/> -->
               <rect x="5" y="5" width="20" height="20" fill="#336699"/>"##,
        ));
        let a = a.unwrap();
        assert_eq!(a.solids(), 1);
        assert_eq!(a.contours[0].color.map(|c| (c[0] * 255.0).round() as u32), Some(0x33));
        assert!(parse("not an svg").is_err());
        assert!(parse(&wrap(r#"<line x1="0" y1="0" x2="5" y2="5"/>"#)).is_err());
    }

    #[test]
    fn unsupported_features_are_reported() {
        let a = parse(&wrap(r#"<rect width="20" height="20" stroke="red"/><text>hello</text><image href="data:abc"/>"#)).unwrap();
        assert!(a.warnings.iter().any(|s| s.contains("stroke")));
        assert!(a.warnings.iter().any(|s| s.contains("text")));
        assert!(a.warnings.iter().any(|s| s.contains("image")));
    }

    #[test]
    fn local_use_repeats_defs_and_preserves_instance_transform() {
        let doc = wrap(
            r##"<defs><g id="motif"><rect width="10" height="20" fill="#336699"/></g></defs>
            <use href="#motif" x="10" y="5"/>
            <use href="#motif" transform="translate(40 5) scale(2)"/>"##,
        );
        let a = parse(&doc).unwrap();
        assert_eq!((a.solids(), a.holes()), (2, 0));
        assert!(a.contours.iter().all(|c| c.color.is_some()));
        let (lo, hi) = extent(&a);
        assert!((hi[0] - lo[0] - 60.0).abs() < 0.01);
    }

    #[test]
    fn local_use_cycles_and_external_references_do_not_expand() {
        let doc = wrap(
            r##"<defs><g id="a"><use href="#b"/></g>
             <g id="b"><use href="#a"/><rect width="8" height="8"/></g></defs>
             <use href="#a"/><use href="https://example.com/evil.svg#x"/>"##,
        );
        let a = parse(&doc).unwrap();
        assert_eq!(a.solids(), 1);
        assert!(parse(&wrap(r##"<use href="file:///etc/passwd#s"/>"##)).is_err());
    }

    #[test]
    fn missing_and_cyclic_references_report_partial_imports() {
        for reference in
            [r##"<use href="#missing"/>"##, r##"<defs><g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g></defs><use href="#a"/>"##]
        {
            assert!(parse(&wrap(reference)).unwrap_err().contains("refers to itself"));
            let artwork = parse(&wrap(&format!(r#"{reference}<rect width="10" height="20"/>"#))).unwrap();
            assert_eq!(artwork.solids(), 1);
            assert_eq!(artwork.warnings.len(), 1, "one warning covers either omission: {:?}", artwork.warnings);
            assert!(artwork.warnings[0].contains("missing or refers to itself"));
        }
        let unused =
            wrap(r##"<defs><g id="unused"><use href="#missing"/></g><g id="empty"/></defs><use href="#empty"/><rect width="10" height="20"/>"##);
        assert!(parse(&unused).unwrap().warnings.is_empty(), "unused definitions and valid empty groups omit no requested shape");
    }

    #[test]
    fn reference_budget_counts_repeated_non_geometry_bytes() {
        let source = wrap(&format!(
            r##"<rect width="10" height="10"/><defs><g id="payload"><!-- {} --></g></defs>{}<rect x="40" width="10" height="10"/>"##,
            "x".repeat(250_000),
            r##"<use href="#payload"/>"##.repeat(180)
        ));
        let ids = collect_ids(&source);
        let (mut limited, mut unresolved) = (false, false);
        let raws = walk(&source, None, &ids, &mut limited, &mut unresolved);
        assert_eq!(raws.len(), 2, "the shapes around the references still load");
        assert!(limited, "180 copies of a 250 kB definition exceed the reference byte budget");
        assert!(!unresolved);
        let artwork = parse(&source).unwrap();
        assert_eq!(artwork.solids(), 2);
        assert!(artwork.warnings.iter().any(|warning| warning.contains("complexity limit")));
    }

    #[test]
    fn referenced_hole_has_matching_extrusion() {
        let a = parse(&wrap(
            r##"<defs><path id="ring" fill-rule="evenodd"
          d="M0 0 H80 V80 H0 Z M20 20 H60 V60 H20 Z"/></defs>
          <use href="#ring"/>"##,
        ))
        .unwrap();
        assert_eq!((a.solids(), a.holes()), (1, 1));
        let (groups, _) = build_group(&a, "ring", &BuildOpts::default(), 64, 8);
        assert_eq!(all_ops(&groups), vec![Op::Add, Op::Subtract]);
    }

    #[test]
    fn namespaced_root_is_accepted_but_an_unrelated_xml_root_is_not() {
        let a = parse(r#"<?xml version="1.0"?><s:svg xmlns:s="http://www.w3.org/2000/svg"><s:rect width="10" height="20"/></s:svg>"#).unwrap();
        assert_eq!(a.solids(), 1);
        assert!(parse(r#"<svg-thumbnail><rect width="10" height="20"/></svg-thumbnail>"#).is_err());
    }

    #[test]
    fn nested_svg_and_symbol_viewports_keep_their_dimensions() {
        let nested = parse(r#"<svg viewBox="0 0 100 100" width="100mm" height="100mm"><svg x="10" y="20" width="40" height="20" viewBox="0 0 10 10" preserveAspectRatio="none"><rect width="10" height="10"/></svg></svg>"#).unwrap();
        assert!((nested.natural_mm[0] - 40.0).abs() < 0.01 && (nested.natural_mm[1] - 20.0).abs() < 0.01);
        let symbol = parse(r##"<svg viewBox="0 0 100 100" width="100mm" height="100mm"><defs><symbol id="tile" viewBox="0 0 10 10" preserveAspectRatio="none"><rect width="10" height="10"/></symbol></defs><use href="#tile" x="10" y="20" width="40" height="20"/></svg>"##).unwrap();
        assert_eq!(nested, symbol);
    }

    #[test]
    fn viewport_alignment_and_slice_are_respected() {
        let tag = next_tag(r#"<svg viewBox="0 0 10 10" width="40" height="20" preserveAspectRatio="xMaxYMin meet"/>"#, &mut 0).unwrap();
        let t = root_transform(&tag, None);
        assert_eq!(t.apply(0.0, 0.0), (20.0, 0.0));
        assert_eq!(t.apply(10.0, 10.0), (40.0, 20.0));
        let tag = next_tag(r#"<svg viewBox="0 0 10 10" width="40" height="20" preserveAspectRatio="xMinYMax slice"/>"#, &mut 0).unwrap();
        let t = root_transform(&tag, None);
        assert_eq!(t.apply(0.0, 0.0), (0.0, -20.0));
        assert_eq!(t.apply(10.0, 10.0), (40.0, 20.0));
    }

    #[test]
    fn nested_svg_viewports_clip_slice_meet_and_visible_overflow() {
        let doc = |inner: &str| format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">{inner}</svg>"#);
        // The browser shows only the inner 100 × 50 viewport of the 100 × 100 sliced picture.
        let slice = parse(&doc(r#"<svg x="50" y="75" width="100" height="50" viewBox="0 0 100 100" preserveAspectRatio="xMidYMid slice">
                 <rect width="100" height="100" fill="black"/></svg>"#))
        .unwrap();
        let (lo, hi) = extent(&slice);
        assert!((hi[0] - lo[0] - 60.0).abs() < 1e-2, "the visible width is 100 units");
        assert!((hi[1] - lo[1] - 30.0).abs() < 1e-2, "slice is clipped to the 100 × 50 viewport, not 100 × 100");
        // meet: the letterboxed content is still clipped to the viewport.
        let meet = parse(&doc(r#"<svg x="50" y="75" width="100" height="50" viewBox="0 0 100 100" preserveAspectRatio="xMidYMid meet">
                 <rect x="-50" y="-50" width="200" height="200"/></svg>"#))
        .unwrap();
        let (lo, hi) = extent(&meet);
        assert!((hi[0] - lo[0] - 60.0).abs() < 1e-2 && (hi[1] - lo[1] - 30.0).abs() < 1e-2, "{} {}", hi[0] - lo[0], hi[1] - lo[1]);
        // overflow="visible" keeps everything the viewBox transform shows.
        let visible =
            parse(&doc(r#"<svg x="50" y="75" width="100" height="50" viewBox="0 0 100 100" preserveAspectRatio="xMidYMid slice" overflow="visible">
                 <rect width="100" height="100" fill="black"/></svg>"#))
            .unwrap();
        let (lo, hi) = extent(&visible);
        assert!((hi[0] - lo[0] - 60.0).abs() < 1e-2 && (hi[1] - lo[1] - 60.0).abs() < 1e-2, "{} {}", hi[0] - lo[0], hi[1] - lo[1]);
    }

    #[test]
    fn style_overrides_percent_colors_and_inherited_fill_are_kept() {
        let a = parse(&wrap(r##"<g fill="none"><rect width="8" height="8" fill="inherit"/><rect x="10" width="8" height="8" style="fill:none; fill:rgb(100%, 50%, 0%)"/></g><rect x="30" width="8" height="8" fill="BLUE"/>"##)).unwrap();
        assert_eq!(a.solids(), 2, "fill=inherit must not turn fill=none into a solid");
        assert_eq!(a.contours[0].color, Some([1.0, 0.5, 0.0]));
        assert_eq!(a.contours[1].color, Some([0.0, 0.0, 1.0]));
        let ring = parse(&wrap(r#"<g fill-rule="evenodd"><path fill-rule="inherit" d="M0 0 H80 V80 H0 Z M20 20 H60 V60 H20 Z"/></g>"#)).unwrap();
        assert_eq!((ring.solids(), ring.holes()), (1, 1));
    }

    #[test]
    fn current_color_and_eight_digit_hex_keep_their_rgb() {
        let a = parse(&wrap(r##"<g color="#00ff00"><rect width="8" height="8" fill="currentColor"/></g><rect x="20" width="8" height="8" fill="#ff000080"/><rect x="40" width="8" height="8" fill="#0f08"/>"##)).unwrap();
        assert_eq!(a.contours[0].color, Some([0.0, 1.0, 0.0]));
        assert_eq!(a.contours[1].color, Some([1.0, 0.0, 0.0]));
        assert_eq!(a.contours[2].color, Some([0.0, 1.0 * 15.0 * 17.0 / 255.0, 0.0]));
    }

    #[test]
    fn transparent_fills_stay_hidden_and_descendants_can_override_fill_opacity() {
        let a = parse(&wrap(r#"<g opacity="0%"><rect width="8" height="8" opacity="1"/></g><rect width="8" height="8" fill="transparent"/><g fill-opacity="0"><rect width="8" height="8"/><rect x="20" width="8" height="8" fill-opacity="100%"/></g>"#)).unwrap();
        assert_eq!(a.solids(), 1);
        let bad =
            parse(&wrap(r#"<g transform="scale(1000000000)"><rect width="8" height="8" visibility="visible"/></g><rect width="8" height="8"/>"#))
                .unwrap();
        assert_eq!(bad.solids(), 1, "visibility cannot make an invalid transform safe");
    }

    #[test]
    fn empty_use_trees_are_bounded_even_when_they_generate_no_points() {
        let mut body = String::from(r#"<rect width="10" height="10"/><defs><g id="n0"/>"#);
        for i in 1..=8 {
            let refs = format!(r##"<use href="#n{}"/>"##, i - 1).repeat(8);
            body.push_str(&format!(r#"<g id="n{i}">{refs}</g>"#));
        }
        body.push_str(r##"</defs><use href="#n8"/>"##);
        let a = parse(&wrap(&body)).unwrap();
        assert_eq!(a.solids(), 1);
        assert!(a.warnings.iter().any(|w| w.contains("complexity limit")));
    }

    #[test]
    fn contour_limit_never_keeps_a_compound_path_without_all_of_its_holes() {
        let mut path = String::from("M0 0 H1000 V1000 H0Z");
        for i in 0..MAX_CONTOURS {
            let (x, y) = (10 + i % 20 * 20, 10 + i / 20 * 20);
            path.push_str(&format!(" M{x} {y} h2 v2 h-2 Z"));
        }
        let a = parse(&wrap(&format!(r#"<rect x="-20" width="10" height="10"/><path fill-rule="evenodd" d="{path}"/>"#))).unwrap();
        assert_eq!((a.solids(), a.holes()), (1, 0));
        assert_eq!(a.dropped, MAX_CONTOURS + 1);
    }

    #[test]
    fn scene_capacity_never_fills_a_missing_hole() {
        let ring = parse(&wrap(r#"<path fill-rule="evenodd" d="M0 0 H80 V80 H0 Z M20 20 H60 V60 H20 Z"/>"#)).unwrap();
        let (g, note) = build_group(&ring, "ring", &BuildOpts::default(), 1, 8);
        assert!(all_shapes(&g).is_empty() && note.is_some());
        let (g, note) = build_group(&ring, "ring", &BuildOpts::default(), 2, 8);
        assert_eq!(all_ops(&g), vec![Op::Add, Op::Subtract]);
        assert!(note.is_none());
        // no group slot for the child means the solid and its hole stay out together
        let (g, note) = build_group(&ring, "ring", &BuildOpts::default(), 2, 1);
        assert!(all_shapes(&g).is_empty() && note.is_some());
    }

    #[test]
    fn skew_scale_bounds_curve_error_in_every_direction() {
        let t = Affine([1.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
        assert!((t.scale() - (1.0 + 5.0_f64.sqrt()) * 0.5).abs() < 1e-12);
        for i in 0..360 {
            let a = (i as f64).to_radians();
            let q = t.apply(a.cos(), a.sin());
            assert!(q.0.hypot(q.1) <= t.scale() + 1e-12);
        }
    }

    #[test]
    fn hostile_input_never_panics() {
        let big = "9".repeat(5000);
        let mut cases: Vec<String> = vec![
            "<svg".into(),
            "<svg><".into(),
            "<svg><path d=\"".into(),
            "<svg><path d=\"M\"/></svg>".into(),
            "<svg><path d=\"M1e999 1e999 L 5 5 Z\"/></svg>".into(),
            "<svg><path d=\"M nan inf L -inf 4 Z\"/></svg>".into(),
            "<svg><path d=\"M0 0 A 0 0 0 5 5 10 10 Z\"/></svg>".into(),
            "<svg><path d=\"M0 0 A 1e-300 1e300 0 1 1 1e300 1e-300 Z\"/></svg>".into(),
            "<svg><path d=\"M0 0 C 1e300 1e300 -1e300 1e300 5 5 Z\"/></svg>".into(),
            "<svg><path d=\"zzzz LLL M 1 2 3\"/></svg>".into(),
            format!("<svg><rect width=\"{big}\" height=\"{big}\"/></svg>"),
            "<svg viewBox=\"0 0 0 0\" width=\"-5\"><circle r=\"1e300\"/></svg>".into(),
            "<svg><g transform=\"scale(1e300) matrix(1 2 3)\"><rect width=\"5\" height=\"5\"/></g></svg>".into(),
            "<svg><g transform=\"rotate(\"><rect width=\"5\" height=\"5\"/></g></svg>".into(),
            "<!DOCTYPE svg [ <!ENTITY a \"b\" ><svg><rect width=\"5\" height=\"5\"/></svg>".into(),
            "<svg><rect width=\"5\" height=\"5\" x=\"\u{3042}\u{3044}\" style=\"\u{3042}:\"/><style>a{}</svg>".into(),
            "<svg><polygon points=\"1,2,3\"/><polyline points=\"\"/></svg>".into(),
            "</svg></g></g><svg>".into(),
        ];
        cases.push(format!("<svg>{}<rect width=\"5\" height=\"5\"/>{}</svg>", "<g>".repeat(50_000), "</g>".repeat(50_000)));
        cases.push(format!("<svg><path d=\"M0 0 {}\"/></svg>", "L 1 1 ".repeat(60_000)));
        cases.push(format!("<svg>{}</svg>", "<circle cx=\"5\" cy=\"5\" r=\"3\"/>".repeat(20_000)));
        cases.push(format!("<svg><path d=\"M0 0 {}\"/></svg>", "C 1 1 2 2 3 3 ".repeat(10_000)));
        cases.push(format!("<svg><path d=\"{}\"/></svg>", "M0 0 L1 0 L1 1 Z ".repeat(20_000)));
        for c in &cases {
            let _ = parse(c);
        }
        assert!(parse_bytes(&vec![b'<'; MAX_BYTES + 1]).is_err());
        let _ = parse_bytes(&[0xff, 0xfe, b'<', 0x80]);
        // deep nesting still reads the shape inside
        let deep = format!("<svg>{}<rect width=\"5\" height=\"5\"/>{}</svg>", "<g>".repeat(40), "</g>".repeat(40));
        assert!(parse(&deep).is_ok());
        // many shapes are capped, not fatal
        let many: String = (0..2000).map(|i| format!("<rect x=\"{}\" y=\"{}\" width=\"3\" height=\"3\"/>", i % 50 * 5, i / 50 * 5)).collect();
        let a = parse(&format!("<svg>{many}</svg>")).unwrap();
        assert!(a.contours.len() <= MAX_CONTOURS && a.dropped > 0);
        let (groups, note) = build_group(&a, "x", &BuildOpts::default(), 64, 64);
        assert!(all_shapes(&groups).len() == 64 && note.is_some());
    }

    #[test]
    fn build_options_are_made_safe() {
        let a = parse(SAMPLE_SVG).unwrap();
        let o = BuildOpts { target_mm: f32::NAN, height_mm: f32::INFINITY, round_mm: -4.0, flip: true, at: [f32::NAN, 1e30] };
        let (groups, _) = build_group(&a, "x", &o, 64, 64);
        let mut s = crate::model::Scene::default();
        s.groups.extend(groups.clone());
        let before = s.clone();
        s.sanitize();
        assert_eq!(s, before, "already within the limits");
        assert!(all_shapes(&groups).iter().all(|s| s.round >= 0.0 && s.pos.iter().all(|v| v.is_finite())));
    }
}
