//! Puffy Text: installed TTF/OTF/TTC outlines -> the EXISTING SVG contour classifier ->
//! editable groups of the EXISTING SDF extrusions and subtractive counters.
//!
//! A glyph's constituent contours stay inside one independent group. No font binary is
//! copied into the project. All geometry is saved as ordinary shapes, so it remains
//! renderable/printable even if a font disappears after the project is saved.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Op, Pick, Scene};
use crate::sdf::{self, V3};
use crate::svg::{self, Artwork, BuildOpts};

pub const MAX_CHARS: usize = 24;
const MAX_FONT_BYTES: u64 = 40 * 1024 * 1024;
const MAX_PEN_COMMANDS: usize = 15_000;
const MAX_GLYPH_SVG_BYTES: usize = 200_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

impl Align {
    pub fn label(self) -> &'static str {
        match self {
            Align::Left => "左揃え",
            Align::Center => "中央揃え",
            Align::Right => "右揃え",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub text: String,
    /// Absolute system/user font file location; only outlines are stored in the design.
    pub font_path: String,
    /// Face index inside .ttc collections.
    pub face_index: u32,
    /// Font em-height, mm.
    pub size_mm: f32,
    /// Height of the uninflated extrusion, mm.
    pub thickness_mm: f32,
    /// Signed SDF inflation of each solid; cutters use the opposite sign.
    pub puff_mm: f32,
    /// Edge softening through the existing extrusion rounding.
    pub corner_mm: f32,
    pub tracking_mm: f32,
    pub align: Align,
    /// Position of the text alignment reference in XY, mm.
    pub anchor: [f32; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            text: "ぷっくり".into(),
            font_path: String::new(),
            face_index: 0,
            size_mm: 30.0,
            thickness_mm: 8.0,
            puff_mm: 0.8,
            corner_mm: 1.5,
            tracking_mm: 1.5,
            align: Align::Center,
            anchor: [0.0; 2],
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.text = self.text.chars().filter(|c| !c.is_control()).take(MAX_CHARS).collect();
        self.font_path = self.font_path.chars().take(1024).collect();
        self.face_index = self.face_index.min(32);
        let sane = |v: f32, fallback: f32, lo: f32, hi: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { fallback }
        };
        self.size_mm = sane(self.size_mm, 30.0, 4.0, 100.0);
        self.thickness_mm = sane(self.thickness_mm, 8.0, 0.8, 60.0);
        self.puff_mm = sane(self.puff_mm, 0.8, 0.0, 3.0);
        self.corner_mm = sane(self.corner_mm, 1.5, 0.0, 10.0);
        self.tracking_mm = sane(self.tracking_mm, 1.5, -3.0, 30.0);
        for v in &mut self.anchor {
            *v = sane(*v, 0.0, -900.0, 900.0);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.text.is_empty() || self.text.chars().count() > MAX_CHARS || self.text.chars().all(char::is_whitespace) {
            return Err(format!("空白だけの文字列は使えません（最大{MAX_CHARS}文字）"));
        }
        if self.text.chars().any(|c| c.is_control()) {
            return Err("改行や制御文字には対応していません".into());
        }
        if self.font_path.is_empty() {
            return Err("日本語・英語を含むフォント（TTF/OTF/TTC）を選んでください".into());
        }
        if self.font_path.len() > 1024 {
            return Err("フォントのパスが長すぎます".into());
        }
        if !self.size_mm.is_finite()
            || !(4.0..=100.0).contains(&self.size_mm)
            || !self.thickness_mm.is_finite()
            || !(0.8..=60.0).contains(&self.thickness_mm)
            || !self.puff_mm.is_finite()
            || !(0.0..=3.0).contains(&self.puff_mm)
            || self.puff_mm > self.size_mm * 0.08
            || !self.corner_mm.is_finite()
            || !(0.0..=10.0).contains(&self.corner_mm)
            || !self.tracking_mm.is_finite()
            || !(-3.0..=30.0).contains(&self.tracking_mm)
            || self.anchor.iter().any(|n| !n.is_finite() || n.abs() > 900.0)
        {
            return Err("文字の大きさ・厚さ・ふくらみ・位置を安全な範囲にしてください".into());
        }
        Ok(())
    }
}

/// Persisted once per visible character group. Rebuilding never needs font bytes
/// if the user only wants to move/rotate/export the already baked shapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LetterMeta {
    pub run_id: u32,
    pub index: usize,
    pub settings: Settings,
    /// Centre of the GENERATED glyph before manual transformations. Delta to the
    /// current group centre is the user's per-letter offset, preserved on regeneration.
    pub base_centre: V3,
}

impl LetterMeta {
    pub fn sanitize(&mut self) {
        self.index = self.index.min(MAX_CHARS);
        self.settings.sanitize();
        for n in &mut self.base_centre {
            if !n.is_finite() {
                *n = 0.0;
            }
            *n = (*n).clamp(-5000.0, 5000.0);
        }
    }
}

/// SVG's safe, bounded parser is already responsible for flattening outlines and
/// classifying nested contours. Emit only numeric SVG path commands, never font text.
#[derive(Default)]
struct SvgPen {
    d: String,
    count: usize,
    invalid: bool,
}

impl SvgPen {
    fn cmd(&mut self, name: char, coordinates: &[f32]) {
        if self.count >= MAX_PEN_COMMANDS || coordinates.iter().any(|v| !v.is_finite() || v.abs() > 1.0e6) {
            self.invalid = true;
            return;
        }
        self.count += 1;
        self.d.push(name);
        for n in coordinates {
            let _ = write!(&mut self.d, "{n:.4} ");
        }
    }
}

impl OutlineBuilder for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cmd('M', &[x, y]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cmd('L', &[x, y]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.cmd('Q', &[x1, y1, x, y]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.cmd('C', &[x1, y1, x2, y2, x, y]);
    }
    fn close(&mut self) {
        self.cmd('Z', &[]);
    }
}

fn font_bytes(path: &str) -> Result<Vec<u8>, String> {
    let file = Path::new(path);
    let allowed = file.extension().and_then(|s| s.to_str()).is_some_and(|s| ["ttf", "ttc", "otf"].iter().any(|a| s.eq_ignore_ascii_case(a)));
    if !allowed {
        return Err("TTF / TTC / OTF のフォントを選んでください".into());
    }
    let len = std::fs::metadata(file).map_err(|_| "フォントが見つかりません。以前の輪郭はそのまま利用できます".to_string())?.len();
    if len == 0 || len > MAX_FONT_BYTES {
        return Err("フォントが大きすぎるか空です（40MBまで）".into());
    }
    std::fs::read(file).map_err(|_| "フォントを読み込めませんでした".to_string())
}

fn outlined(face: &Face<'_>, id: GlyphId) -> Result<Artwork, String> {
    let mut pen = SvgPen::default();
    // ttf-parser may produce partial segments before rejecting a malformed glyph.
    // Require an actual successful outline result, not just some pen commands.
    if face.outline_glyph(id, &mut pen).is_none() {
        return Err("フォントの文字輪郭を安全に読み込めませんでした".into());
    }
    if pen.count == 0 {
        return Err("この文字には閉じた輪郭がありません".into());
    }
    if pen.invalid || pen.count >= MAX_PEN_COMMANDS || pen.d.len() > MAX_GLYPH_SVG_BYTES {
        return Err("文字の輪郭が複雑すぎます（上限を超えました）".into());
    }
    // Even-odd parity keeps O, B, 田, 日 etc. hollow regardless of font winding.
    // The SVG importer does adaptive curve flattening and bounded contour simplification.
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000"><path fill-rule="evenodd" d="{}"/></svg>"#, pen.d);
    let art = svg::parse(&svg)?;
    if art.dropped != 0 {
        return Err(format!("細かい輪郭を{}個省略するため、このフォント・文字は安全に立体化できません", art.dropped));
    }
    Ok(art)
}

fn mean_centre(group: &Group) -> V3 {
    let n = group.shapes.len().max(1) as f32;
    let mut out = [0.0; 3];
    for s in &group.shapes {
        for (o, p) in out.iter_mut().zip(s.pos) {
            *o += p / n;
        }
    }
    out
}

/// A conservative inscribed-radius sample for each hole. We reject inflation that
/// could fill a hole rather than silently changing the letter's reading.
fn hole_clearance(poly: &[[f32; 2]]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    for v in poly {
        for k in 0..2 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let mut best = 0.0f32;
    for i in 1..20 {
        for j in 1..20 {
            let at = [lo[0] + (hi[0] - lo[0]) * i as f32 / 20.0, lo[1] + (hi[1] - lo[1]) * j as f32 / 20.0];
            best = best.max(-sdf::polygon(poly, at));
        }
    }
    best
}

/// Group of 3D contours for one glyph; existing extrusions handle all graphics and export.
fn glyph_group(art: &Artwork, ch: char, index: usize, settings: &Settings, center: [f32; 2], glyph_mm: f32) -> Result<Group, String> {
    let name = format!("Puffy {ch} ({})", index + 1);
    let options = BuildOpts {
        target_mm: 60.0, // normalize first: avoids SVG import's minimum sizing of tiny glyphs
        height_mm: settings.thickness_mm,
        round_mm: settings.corner_mm.min(settings.thickness_mm * 0.5),
        flip: true, // SVG maps Y downward; font outlines have Y upward
        at: center,
    };
    let (mut parts, note) = svg::build_group(art, &name, &options, MAX_SHAPES, MAX_GROUPS);
    // one letter is one group: the solids and the counters of a glyph go together
    let mut parts = parts.drain(..);
    let Some(mut group) = parts.next() else {
        return Err(format!("「{ch}」の輪郭を立体にできませんでした"));
    };
    for part in parts {
        group.shapes.extend(part.shapes);
    }
    if note.is_some() || group.shapes.is_empty() {
        return Err(format!("「{ch}」の輪郭を立体にできませんでした"));
    }
    let factor = glyph_mm / 60.0;
    for sh in &mut group.shapes {
        for pt in &mut sh.outline {
            pt[0] *= factor;
            pt[1] *= factor;
            if !pt.iter().all(|n| n.is_finite() && n.abs() <= 2000.0) {
                return Err(format!("「{ch}」の輪郭が大きすぎます"));
            }
        }
        let hole = sh.op == Op::Subtract;
        if hole && hole_clearance(&sh.outline) <= settings.puff_mm + settings.corner_mm * 0.5 + 0.15 {
            return Err(format!("「{ch}」の穴がつぶれます。ふくらみを小さくしてください"));
        }
        sh.pos[2] = settings.thickness_mm * 0.5 + settings.puff_mm;
        if hole {
            // A through-cutter must reach beyond BOTH expanded top and bottom faces.
            sh.size[0] = settings.thickness_mm + 4.0 * settings.puff_mm + 4.0;
            sh.params[0] = -settings.puff_mm;
        } else {
            sh.params[0] = settings.puff_mm;
        }
        sh.blend = 0.0;
    }
    group.blend = 0.0;
    Ok(group)
}

/// Keep per-letter translation, rotation, uniform scaling and appearance on regeneration.
/// The generated base centroid is separate from the adjusted world-space letter.
fn keep_manual_transform(group: &mut Group, old: &Group) {
    let Some(meta) = old.puffy_text.as_ref() else { return };
    let Some(first) = old.shapes.first() else { return };
    let rot = first.rot;
    let scale = first.scale;
    let r = sdf::rotation(rot);
    let old_mid = mean_centre(old);
    let new_mid = mean_centre(group);
    let offset = sdf::sub(old_mid, meta.base_centre);
    for sh in &mut group.shapes {
        let relative = sdf::sub(sh.pos, new_mid);
        let scaled = [relative[0] * scale[0], relative[1] * scale[1], relative[2] * scale[2]];
        sh.pos = sdf::add(sdf::add(new_mid, offset), [sdf::dot(r[0], scaled), sdf::dot(r[1], scaled), sdf::dot(r[2], scaled)]);
        sh.rot = rot;
        sh.scale = scale;
        if sh.op == Op::Add {
            sh.material = first.material;
        }
    }
}

/// Atomically build or regenerate a run. The original geometry stays untouched
/// if a font/glyph is missing, geometry invalid, or scene/group limits are reached.
pub fn apply(scene: &mut Scene, settings: &Settings, editing_run: Option<u32>) -> Result<Pick, String> {
    settings.validate()?;
    let bytes = font_bytes(&settings.font_path)?;
    let face = Face::parse(&bytes, settings.face_index).map_err(|_| "このフォントを読み込めません（TTCの番号も確認してください）".to_string())?;
    let upm = f32::from(face.units_per_em()).max(1.0);
    let size = settings.size_mm / upm;

    let chars: Vec<char> = settings.text.chars().collect();
    let mut glyphs: Vec<(char, Option<GlyphId>, f32)> = Vec::with_capacity(chars.len());
    let mut total = 0.0f32;
    for (i, &ch) in chars.iter().enumerate() {
        let glyph = face.glyph_index(ch);
        if glyph.is_none() && !ch.is_whitespace() {
            return Err(format!("選択したフォントに「{ch}」がありません。日本語対応フォントを選んでください"));
        }
        let advance = glyph.and_then(|id| face.glyph_hor_advance(id)).map_or(upm * 0.5, f32::from) * size;
        glyphs.push((ch, glyph, advance));
        total += advance;
        if i + 1 < chars.len() {
            total += settings.tracking_mm;
        }
    }
    if total <= 0.0 || total > 3000.0 {
        return Err("文字列の幅が大きすぎるか、小さすぎます".into());
    }
    let base = match settings.align {
        Align::Left => 0.0,
        Align::Center => -total * 0.5,
        Align::Right => -total,
    };

    let old_ids: Vec<usize> =
        scene.groups.iter().enumerate().filter_map(|(i, g)| g.puffy_text.as_ref().filter(|m| Some(m.run_id) == editing_run).map(|_| i)).collect();
    if editing_run.is_some() && old_ids.is_empty() {
        return Err("再編集する文字列が見つかりません".into());
    }
    // References to group UIDs must not be invalidated by replacement.
    if old_ids.iter().any(|&i| {
        let uid = scene.groups[i].uid;
        uid != 0 && scene.groups.iter().any(|g| g.instance_of == Some(uid))
    }) {
        return Err("リンクされた文字グループは、リンクを解除してから再編集してください".into());
    }

    let new_run = match editing_run {
        Some(id) => id,
        None => scene
            .groups
            .iter()
            .filter_map(|g| g.puffy_text.as_ref().map(|m| m.run_id))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("文字列IDの上限です")?,
    };
    let old: Vec<(usize, &Group)> = old_ids.iter().filter_map(|&i| scene.groups.get(i).map(|g| (i, g))).collect();
    let mut built = Vec::new();
    let mut cursor = base;
    for (index, (ch, id, advance)) in glyphs.into_iter().enumerate() {
        if let Some(id) = id {
            if let Some(bounds) = face.glyph_bounding_box(id) {
                let art = outlined(&face, id).map_err(|reason| format!("「{ch}」: {reason}"))?;
                let width = f32::from(bounds.x_max) - f32::from(bounds.x_min);
                let height = f32::from(bounds.y_max) - f32::from(bounds.y_min);
                let centre = [
                    settings.anchor[0] + cursor + (f32::from(bounds.x_min) + f32::from(bounds.x_max)) * 0.5 * size,
                    settings.anchor[1] + (f32::from(bounds.y_min) + f32::from(bounds.y_max)) * 0.5 * size,
                ];
                let mut group = glyph_group(&art, ch, index, settings, centre, width.max(height) * size)?;
                let baseline = mean_centre(&group);
                if let Some((_, previous)) = old.iter().find(|(_, group)| group.puffy_text.as_ref().is_some_and(|meta| meta.index == index)) {
                    keep_manual_transform(&mut group, previous);
                    group.visible = previous.visible;
                    group.op = previous.op;
                }
                group.puffy_text = Some(LetterMeta { run_id: new_run, index, settings: settings.clone(), base_centre: baseline });
                built.push(group);
            } else if !ch.is_whitespace() {
                return Err(format!("「{ch}」に輪郭がありません"));
            }
        }
        cursor += advance + if index + 1 < chars.len() { settings.tracking_mm } else { 0.0 };
    }
    if built.is_empty() {
        return Err("立体化できる文字がありません".into());
    }
    let removed_shapes: usize = old.iter().map(|(_, g)| g.shapes.len()).sum();
    let remaining_shapes = scene.shape_count().saturating_sub(removed_shapes);
    let new_shapes: usize = built.iter().map(|g| g.shapes.len()).sum();
    if scene.groups.len() - old_ids.len() + built.len() > MAX_GROUPS || remaining_shapes + new_shapes > MAX_SHAPES {
        return Err(format!("形・グループの上限を超えます（形{MAX_SHAPES}、グループ{MAX_GROUPS}まで）。文字数を減らしてください"));
    }
    let insertion = old_ids.first().copied().unwrap_or(scene.groups.len());
    let mut next = scene.clone();
    next.groups.retain(|g| !g.puffy_text.as_ref().is_some_and(|m| m.run_id == new_run));
    // Do not reorder any unrelated groups.
    let at = insertion.min(next.groups.len());
    next.groups.splice(at..at, built);
    *scene = next;
    Ok(Pick::Group(at))
}

/// Return the text settings for either a glyph group or one of its constituent shapes.
pub fn selected(scene: &Scene, pick: Option<Pick>) -> Option<&LetterMeta> {
    let g = match pick? {
        Pick::Group(g) | Pick::Shape(g, _) => g,
    };
    scene.groups.get(g)?.puffy_text.as_ref()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::sdf::Field;

    #[test]
    fn ring_counter_survives_puffing_sdf_save_and_reopen() {
        let art = svg::parse(
            r#"<svg viewBox="0 0 100 100"><path fill-rule="evenodd" d="M0 0 L100 0 L100 100 L0 100 Z M30 30 L70 30 L70 70 L30 70 Z"/></svg>"#,
        )
        .expect("SVG");
        assert_eq!((art.solids(), art.holes()), (1, 1));
        let mut settings = Settings::default();
        settings.puff_mm = 0.8;
        let mut group = glyph_group(&art, 'O', 0, &settings, [0.0, 0.0], 30.0).expect("3D O");
        assert_eq!(group.shapes.len(), 2);
        let inside = [-10.0, 0.0, settings.thickness_mm * 0.5];
        let counter = [0.0, 0.0, settings.thickness_mm * 0.5];
        let scene_with = |g| {
            let mut scene = Scene::default();
            scene.groups = vec![g];
            scene
        };
        let scene = scene_with(group.clone());
        let field = Field::new(&scene);
        assert!(field.distance(inside) < 0.0, "letter body exists");
        assert!(field.distance(counter) > 0.0, "the O counter is open");
        assert!(field.distance([0.0, 0.0, 0.0]) > 0.0, "counter is through, including bottom");
        group.puffy_text = Some(LetterMeta { run_id: 1, index: 0, settings, base_centre: mean_centre(&group) });
        let original = scene_with(group);
        let restored = Scene::from_json(&original.to_json()).expect("roundtrip");
        assert_eq!(restored, original);
        assert!(Field::new(&restored).distance(counter) > 0.0);
    }

    #[test]
    fn invalid_settings_and_too_much_puff_never_mutate_a_scene() {
        let mut scene = Scene::default();
        let original = scene.clone();
        let settings = Settings::default();
        assert!(apply(&mut scene, &settings, None).is_err(), "font choice is mandatory");
        assert_eq!(scene, original);
        let art = svg::parse(
            r#"<svg viewBox="0 0 100 100"><path fill-rule="evenodd" d="M0 0 L100 0 L100 100 L0 100 Z M45 45 L55 45 L55 55 L45 55 Z"/></svg>"#,
        )
        .expect("SVG");
        let mut opts = settings;
        opts.puff_mm = 3.0;
        assert!(glyph_group(&art, 'O', 0, &opts, [0.0, 0.0], 30.0).is_err(), "tiny counter must not silently close");
        assert_eq!(scene, original);
    }

    #[test]
    fn individual_letter_transform_survives_rebuild() {
        let art = svg::parse(r#"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="100" height="100"/></svg>"#).expect("SVG");
        let settings = Settings::default();
        let mut original = glyph_group(&art, 'A', 0, &settings, [10.0, 0.0], 30.0).expect("A");
        let baseline = mean_centre(&original);
        original.puffy_text = Some(LetterMeta { run_id: 10, index: 0, settings: settings.clone(), base_centre: baseline });
        for sh in &mut original.shapes {
            sh.pos[0] += 7.0;
            sh.pos[1] -= 3.0;
            sh.rot[2] = 25.0;
        }
        let mut rebuilt = glyph_group(&art, 'A', 0, &settings, [12.0, 2.0], 33.0).expect("rebuilt");
        let fresh = mean_centre(&rebuilt);
        keep_manual_transform(&mut rebuilt, &original);
        let got = mean_centre(&rebuilt);
        assert!((got[0] - fresh[0] - 7.0).abs() < 1.0e-4);
        assert!((got[1] - fresh[1] + 3.0).abs() < 1.0e-4);
        assert!(rebuilt.shapes.iter().all(|s| s.rot[2] == 25.0));
    }

    /// Requires a known-licensed installed font on the test machine. No font is bundled.
    #[test]
    #[ignore = "requires a known-installed CJK font on the test runner"]
    fn if_an_installed_cjk_font_exists_japanese_english_can_be_regenerated() {
        let Some(path) = crate::fonts::find_cjk_font_path() else { return };
        let mut settings = Settings::default();
        settings.font_path = path.to_string_lossy().into_owned();
        settings.text = "A田O".into();
        settings.puff_mm = 0.0;
        let mut scene = Scene::default();
        scene.groups.clear();
        let result = apply(&mut scene, &settings, None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(scene.groups.len(), 3);
        let saved = Scene::from_json(&scene.to_json()).expect("save/read");
        assert_eq!(saved, scene);
        let run = selected(&saved, Some(Pick::Group(0))).unwrap().run_id;
        settings.tracking_mm += 2.0;
        assert!(apply(&mut scene, &settings, Some(run)).is_ok(), "re-edit with same font");
        assert_eq!(scene.groups.len(), 3);
    }

    /// This runs only when explicitly requested because sparse meshing is expensive.
    #[test]
    #[ignore]
    fn inflated_ring_is_watertight_after_mesh_export() {
        let art = svg::parse(
            r#"<svg viewBox="0 0 100 100"><path fill-rule="evenodd" d="M0 0 L100 0 L100 100 L0 100 Z M30 30 L70 30 L70 70 L30 70 Z"/></svg>"#,
        )
        .unwrap();
        let settings = Settings::default();
        let mut scene = Scene::default();
        scene.groups = vec![glyph_group(&art, 'O', 0, &settings, [0.0, 0.0], 30.0).unwrap()];
        let field = Field::new(&scene);
        let progress = crate::export_mesh::Progress::new();
        let mesh = crate::export_mesh::mesh_sparse(
            &|point| field.distance(point),
            [-19.0, -19.0, -3.0],
            [19.0, 19.0, 17.0],
            0.6,
            1,
            &progress.slice(0.0, 1.0),
        )
        .expect("mesh");
        assert!(solidcraft_geom::melt::is_watertight(&mesh.mesh), "3D letter must export watertight");
    }
}
