//! Editable, printable chess pieces built from existing SDF/Revolve/Extrude primitives.
//! Every piece is a separate ordinary scene Group, with no baked mesh or new shader primitive.
//! A small marker on the first shape preserves the preset's parameters across save/reload.

use std::f32::consts::{PI, TAU};

use crate::model::{Group, Kind, Material, MAX_GROUPS, MAX_SHAPES, Op, Scene, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn,
}
impl Piece {
    pub const ALL: [Piece; 6] = [Piece::King, Piece::Queen, Piece::Rook, Piece::Bishop, Piece::Knight, Piece::Pawn];
    pub fn key(self) -> &'static str {
        match self {
            Piece::King => "K", Piece::Queen => "Q", Piece::Rook => "R",
            Piece::Bishop => "B", Piece::Knight => "N", Piece::Pawn => "P",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Piece::King => "キング", Piece::Queen => "クイーン", Piece::Rook => "ルーク",
            Piece::Bishop => "ビショップ", Piece::Knight => "ナイト", Piece::Pawn => "ポーン",
        }
    }
    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == s)
    }
    fn board_height(self) -> f32 {
        match self {
            Piece::King => 1.0, Piece::Queen => 0.90, Piece::Rook => 0.72,
            Piece::Bishop => 0.79, Piece::Knight => 0.78, Piece::Pawn => 0.57,
        }
    }
    fn board_diameter(self) -> f32 {
        match self {
            Piece::King | Piece::Queen => 1.0, Piece::Rook => 0.90,
            Piece::Bishop => 0.87, Piece::Knight => 0.89, Piece::Pawn => 0.70,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Design {
    Classic,
    Minimal,
    Organic,
}
impl Design {
    pub const ALL: [Design; 3] = [Design::Classic, Design::Minimal, Design::Organic];
    pub fn key(self) -> &'static str {
        match self {
            Design::Classic => "C", Design::Minimal => "M", Design::Organic => "O",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Design::Classic => "クラシック", Design::Minimal => "ミニマル", Design::Organic => "有機的",
        }
    }
    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { White, Black }
impl Side {
    pub const ALL: [Side; 2] = [Side::White, Side::Black];
    pub fn key(self) -> &'static str { if self == Side::White { "W" } else { "D" } }
    pub fn name(self) -> &'static str { if self == Side::White { "白" } else { "黒" } }
    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub piece: Piece,
    pub side: Side,
    pub design: Design,
    /// Height of an individual piece, or of the king when generating a complete set.
    pub height_mm: f32,
    pub diameter_mm: f32,
    pub ornament: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Settings { piece: Piece::King, side: Side::White, design: Design::Classic, height_mm: 64.0, diameter_mm: 22.0, ornament: 0.6 }
    }
}
impl Settings {
    pub fn safe(self) -> Self {
        let finite = |v: f32, default: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
        Settings {
            height_mm: finite(self.height_mm, 64.0, 14.0, 110.0),
            diameter_mm: finite(self.diameter_mm, 22.0, 14.0, 48.0),
            ornament: finite(self.ornament, 0.6, 0.0, 1.0),
            ..self
        }
    }
}

fn material(side: Side) -> Material {
    if side == Side::White {
        Material::plain([0.91, 0.87, 0.76], 0.38, 0.0)
    } else {
        Material::plain([0.15, 0.18, 0.23], 0.44, 0.0)
    }
}

/// First-shape marker, intentionally stored in existing serializable Shape.name, so that
/// a selected set piece can be regenerated with the same parameters after loading a file.
fn marker(s: &Settings) -> String {
    format!("ChessPreset:v1:{}:{}:{}:{:.3}:{:.3}:{:.3}",
        s.piece.key(), s.side.key(), s.design.key(), s.height_mm, s.diameter_mm, s.ornament)
}
pub fn preset_of(group: &Group) -> Option<Settings> {
    let name = group.shapes.first()?.name.as_str();
    let mut p = name.split(':');
    if p.next()? != "ChessPreset" || p.next()? != "v1" { return None; }
    Some(Settings {
        piece: Piece::from_key(p.next()?)?,
        side: Side::from_key(p.next()?)?,
        design: Design::from_key(p.next()?)?,
        height_mm: p.next()?.parse::<f32>().ok()?,
        diameter_mm: p.next()?.parse::<f32>().ok()?,
        ornament: p.next()?.parse::<f32>().ok()?,
    }.safe())
}

fn push(g: &mut Group, mut shape: Shape, name: &str) {
    shape.name = name.into();
    shape.blend = 0.0; // hard SDF union of overlapping, closed solids
    shape.op = Op::Add;
    g.shapes.push(shape);
}
fn ball(g: &mut Group, m: Material, name: &str, pos: [f32; 3], r: f32) {
    let mut sh = Shape::new(Kind::Sphere, pos, m);
    sh.size = [r.max(0.5), r.max(0.5), r.max(0.5)];
    push(g, sh, name);
}
fn block(g: &mut Group, m: Material, name: &str, pos: [f32; 3], size: [f32; 3], rounding: f32) {
    let mut sh = Shape::new(Kind::Box, pos, m);
    sh.size = size;
    sh.round = rounding.max(0.0).min(size.into_iter().fold(f32::INFINITY, f32::min) * 0.20);
    push(g, sh, name);
}
/// Make a full 360-degree revolution from radius/z coordinates in absolute model height.
fn revolve(g: &mut Group, m: Material, name: &str, at: [f32; 2], profile: &[[f32; 2]]) {
    let low = profile.iter().map(|v| v[1]).fold(f32::INFINITY, f32::min);
    let high = profile.iter().map(|v| v[1]).fold(f32::NEG_INFINITY, f32::max);
    let middle = (low + high) * 0.5;
    let mut sh = Shape::new(Kind::Revolve, [at[0], at[1], middle], m);
    sh.outline = profile.iter().map(|p| [p[0], p[1] - middle]).collect();
    sh.params[0] = 360.0;
    push(g, sh, name);
}

/// Positive radius, rising Z; solid of revolution, including both horizontal end caps.
/// Decorations stay on the profile: no thin, disconnected ring objects.
fn pedestal(r: f32, top: f32, design: Design, ornament: f32) -> Vec<[f32; 2]> {
    let mut p = vec![[0.0, 0.0]];
    match design {
        Design::Classic => {
            let lip = 0.04 * ornament;
            p.extend_from_slice(&[
                [0.86*r, 0.0], [(0.95+lip)*r, 0.025*top], [(0.95+lip)*r, 0.090*top],
                [0.86*r, 0.11*top], [0.83*r, 0.16*top], [0.56*r, 0.25*top],
                [0.44*r, 0.42*top], [0.46*r, 0.56*top], [0.55*r, 0.73*top],
                [(0.65+lip)*r, 0.78*top], [(0.65+lip)*r, 0.84*top],
                [0.58*r, 0.88*top], [0.50*r, top],
            ]);
        }
        Design::Minimal => {
            p.extend_from_slice(&[
                [0.91*r, 0.0], [0.91*r, 0.09*top], [0.47*r, 0.24*top],
                [0.43*r, 0.76*top], [0.55*r, 0.94*top], [0.52*r, top],
            ]);
        }
        Design::Organic => {
            p.push([0.88*r, 0.0]);
            for i in 1..=22 {
                let t = i as f32 / 22.0;
                let base = 0.49 + 0.37*(1.0-t).powi(2) + 0.08*t;
                let waves = ornament * 0.10 * (t * TAU * 3.0).sin();
                p.push([(r*(base + waves)).max(r*0.25), top*t]);
            }
        }
    }
    p.push([0.0, top]);
    p
}

fn add_king(g: &mut Group, m: Material, at: [f32; 2], h: f32, r: f32, s: Settings) {
    // The cross sinks into the solid foot, and both bars overlap each other.
    let bar = (r*0.28 + 0.045*h*s.ornament).max(1.2);
    let upright_z = 0.885*h;
    block(g, m, "十字・たて", [at[0],at[1],upright_z], [bar,bar,0.23*h], bar*0.13);
    block(g, m, "十字・よこ", [at[0],at[1],0.89*h], [r*1.04,bar,bar], bar*0.12);
    if s.design == Design::Classic && s.ornament > 0.15 {
        revolve(g, m, "王冠の台座", at, &[
            [0.0,0.73*h],[0.49*r,0.73*h],[0.57*r,0.76*h],
            [0.58*r,0.82*h],[0.38*r,0.83*h],[0.0,0.83*h],
        ]);
    }

}
fn add_queen(g: &mut Group, m: Material, at: [f32; 2], h: f32, r: f32, s: Settings) {
    revolve(g, m, "女王の冠", at, &[
        [0.0,0.77*h],[0.49*r,0.77*h],[0.76*r,0.87*h],
        [0.75*r,0.94*h],[0.59*r,0.94*h],[0.42*r,0.87*h],[0.0,0.87*h],
    ]);
    let beads = if s.design == Design::Minimal { 4 } else { 6 };
    let small = (0.030*h + 0.02*r*s.ornament).max(0.65);
    for i in 0..beads {
        let a = TAU*i as f32 / beads as f32;
        ball(g, m, "冠の飾り", [at[0]+a.cos()*0.70*r, at[1]+a.sin()*0.70*r, 0.944*h], small);
    }
    ball(g, m, "女王の宝珠", [at[0], at[1], 0.95*h], 0.050*h);
}
fn add_rook(g: &mut Group, m: Material, at: [f32; 2], h: f32, r: f32, s: Settings) {
    // A recognisable crenellated castle tower, rather than a cylinder icon.
    revolve(g, m, "城壁のへり", at, &[
        [0.0,0.75*h],[0.68*r,0.75*h],[0.84*r,0.80*h],
        [0.84*r,0.88*h],[0.0,0.88*h],
    ]);
    let n = if s.design == Design::Organic { 6 } else { 4 };
    for i in 0..n {
        let a = TAU * (i as f32 + 0.5) / n as f32;
        let pos = [at[0]+a.cos()*0.59*r, at[1]+a.sin()*0.59*r, 0.929*h];
        if s.design == Design::Organic {
            ball(g, m, "有機的な城壁", pos, (0.09*h).min(0.32*r));
        } else {
            block(g, m, "城壁の凸", pos, [0.49*r,0.49*r,0.145*h], r*0.04);
        }
    }
}
fn add_bishop(g: &mut Group, m: Material, at: [f32; 2], h: f32, r: f32, s: Settings) {
    revolve(g, m, "司教帽", at, &[
        [0.0,0.62*h],[0.43*r,0.62*h],[0.61*r,0.74*h],
        [0.51*r,0.84*h],[0.24*r,0.945*h],[0.0,0.986*h],
    ]);
    if s.design != Design::Minimal && s.ornament > 0.1 {
        // A shallow tilted groove on the visible front, not a complete severing cut.
        let mut cut = Shape::new(Kind::Box, [at[0],at[1]-0.49*r,0.82*h], m);
        cut.size = [0.90*r,0.38*r,0.055*h.max(24.0)];
        cut.rot = [0.0,-32.0,0.0];
        cut.op = Op::Subtract;
        cut.blend = 0.0;
        cut.name = "司教帽の斜めの溝".into();
        g.shapes.push(cut);
    }
    ball(g, m, "帽子の先", [at[0],at[1],0.976*h], 0.022*h.max(24.0));
}
fn add_knight(g: &mut Group, m: Material, at: [f32; 2], h: f32, r: f32, s: Settings) {
    // An upright horse's head, not a flat horse image:
    // X/Y polygon is rotated 90 degrees around X to stand in the X/Z plane.
    // The cheek, muzzle and eyes make the head visibly three-dimensional.
    let silhouette: [[f32; 2]; 16] = [
        [-0.62,0.46],[-0.57,0.62],[-0.41,0.78],[-0.46,0.98],
        [-0.18,0.91],[-0.04,0.83],[0.23,0.83],[0.50,0.73],
        [0.56,0.67],[0.33,0.60],[0.16,0.58],[0.21,0.49],
        [0.08,0.46],[-0.10,0.49],[-0.28,0.43],[-0.47,0.42],
    ];
    let centre = 0.70*h;
    let mut head = Shape::new(Kind::Extrude, [at[0], at[1], centre], m);
    head.outline = silhouette.iter().map(|p| [p[0]*r*1.28, p[1]*h-centre]).collect();
    head.size[0] = 1.23*r;
    head.rot = [90.0,0.0,0.0];
    head.round = if s.design == Design::Minimal { 0.3 } else { (r*0.09).min(1.1) };
    push(g, head, "立体的な馬の頭");
    for side in [-1.0_f32, 1.0] {
        let cheek = [at[0]-0.06*r, at[1]+side*0.42*r, 0.705*h];
        ball(g, m, "馬の頬", cheek, 0.40*r);
        let eye = [at[0]+0.15*r, at[1]+side*0.67*r, 0.73*h];
        let rad = (r*0.12).max(0.7);
        ball(g, m, "馬の目", eye, rad);
    }
    ball(g, m, "鼻先", [at[0]+0.42*r,at[1],0.69*h], 0.32*r);
    if s.design != Design::Minimal && s.ornament > 0.2 {
        for i in 0..3 {
            let t = i as f32;
            ball(g, m, "たてがみ", [at[0]-0.54*r,at[1],(0.63+0.085*t)*h], 0.17*r*s.ornament.max(0.4));
        }
    }
}
fn add_pawn(g: &mut Group, m: Material, at: [f32; 2], h: f32) {
    ball(g, m, "ポーンの球", [at[0],at[1],0.80*h], 0.20*h);
}

/// Return a complete editable SDF group placed with its base on Z=0.
pub fn make_piece(settings: Settings, at: [f32; 2], square: &str) -> Group {
    let s = settings.safe();
    let h = s.height_mm;
    let r = s.diameter_mm * 0.5;
    let top = match s.piece {
        Piece::King => 0.77, Piece::Queen => 0.80, Piece::Rook => 0.84,
        Piece::Bishop => 0.67, Piece::Knight => 0.53, Piece::Pawn => 0.65,
    } * h;
    let mut g = Group::new(format!("Chess/{}/{}/{} ({})", s.side.name(), s.piece.name(), square, s.design.name()));
    let m = material(s.side);
    revolve(&mut g, m, &marker(&s), at, &pedestal(r, top, s.design, s.ornament));
    match s.piece {
        Piece::King => add_king(&mut g, m, at, h, r, s),
        Piece::Queen => add_queen(&mut g, m, at, h, r, s),
        Piece::Rook => add_rook(&mut g, m, at, h, r, s),
        Piece::Bishop => add_bishop(&mut g, m, at, h, r, s),
        Piece::Knight => add_knight(&mut g, m, at, h, r, s),
        Piece::Pawn => add_pawn(&mut g, m, at, h),
    }
    g
}
pub fn square_pitch(s: Settings) -> f32 { s.safe().diameter_mm * 1.38 }
pub fn board_width(s: Settings) -> f32 { 8.0*square_pitch(s) + 7.0 }

fn board(s: Settings, at: [f32; 2]) -> Group {
    let pitch = square_pitch(s);
    let w = board_width(s);
    let mut g = Group::new("Chess/盤面 (8×8)");
    let cream = Material::plain([0.83,0.76,0.62],0.57,0.0);
    let dark = Material::plain([0.30,0.34,0.40],0.55,0.0);
    block(&mut g, cream, "盤面・土台", [at[0],at[1],-3.0], [w,w,4.0], 0.6);
    for rank in 0..8 {
        for file in 0..8 {
            if (rank+file) % 2 == 0 { continue; }
            let x = at[0] + (file as f32-3.5)*pitch;
            let y = at[1] + (rank as f32-3.5)*pitch;
            block(&mut g, dark, "濃いマス", [x,y,-0.85], [pitch-0.12,pitch-0.12,0.40], 0.0);
        }
    }
    g
}

fn square(file: usize, rank: usize) -> String {
    format!("{}{}", (b'a' + file as u8) as char, rank+1)
}
fn square_xy(pitch: f32, at: [f32; 2], file: usize, rank: usize) -> [f32; 2] {
    [at[0]+(file as f32-3.5)*pitch, at[1]+(rank as f32-3.5)*pitch]
}

/// 32 independent, individually selectable pieces + a separate optional printable board.
/// The pieces sit at Z=0 and the board ends below them, for separate group exports.
pub fn full_set(s: Settings, at: [f32; 2]) -> Vec<Group> {
    let s = s.safe();
    let pitch = square_pitch(s);
    let mut groups = Vec::with_capacity(33);
    groups.push(board(s, at));
    let back = [Piece::Rook, Piece::Knight, Piece::Bishop, Piece::Queen, Piece::King, Piece::Bishop, Piece::Knight, Piece::Rook];
    for (side, home, pawn) in [(Side::White,0_usize,1_usize), (Side::Black,7,6)] {
        for file in 0..8 {
            for (piece, rank) in [(back[file], home), (Piece::Pawn, pawn)] {
                let piece_s = Settings { piece, side, height_mm: s.height_mm*piece.board_height(), diameter_mm: s.diameter_mm*piece.board_diameter(), ..s };
                groups.push(make_piece(piece_s, square_xy(pitch, at, file, rank), &square(file, rank)));
            }
        }
    }
    groups
}

/// Atomically append the full set. The untouched demo sphere in a brand-new scene is replaced.
pub fn insert_set(scene: &mut Scene, s: Settings, at: [f32; 2]) -> Result<usize, String> {
    let groups = full_set(s, at);
    let fresh = *scene == Scene::default();
    let cur_groups = if fresh { 0 } else { scene.groups.len() };
    let cur_shapes = if fresh { 0 } else { scene.shape_count() };
    let count: usize = groups.iter().map(|g| g.shapes.len()).sum();
    if cur_groups + groups.len() > MAX_GROUPS || cur_shapes + count > MAX_SHAPES {
        return Err(format!("チェスセットを置くには、{}グループ・{}形の空きが必要です", groups.len(), count));
    }
    if fresh { scene.groups.clear(); }
    let first = scene.groups.len();
    scene.groups.extend(groups);
    Ok(first)
}

/// Add a single individually editable piece to the scene.
pub fn insert_piece(scene: &mut Scene, s: Settings, at: [f32; 2]) -> Result<usize, String> {
    let g = make_piece(s, at, "単体");
    if scene.groups.len() >= MAX_GROUPS || scene.shape_count() + g.shapes.len() > MAX_SHAPES {
        return Err("グループ数または形の数が上限です".into());
    }
    let i = scene.groups.len();
    scene.groups.push(g);
    Ok(i)
}

/// Replace only the selected Chess piece, keeping its position and group identity.
/// Returns false rather than overwriting a hand-built/non-chess group.
pub fn rebuild_piece(scene: &mut Scene, group_index: usize, s: Settings) -> Result<(), String> {
    let old = scene.groups.get(group_index).ok_or("駒が見つかりません")?;
    if preset_of(old).is_none() { return Err("チェスの駒を選択してください".into()); }
    let core = old.shapes.first().ok_or("駒の本体が見つかりません")?;
    let at = [core.pos[0], core.pos[1]];
    let label = old.name.clone();
    let mut new = make_piece(s, at, &label);
    let capacity = scene.shape_count() - old.shapes.len() + new.shapes.len();
    if capacity > MAX_SHAPES { return Err("形の数が上限です".into()); }
    new.name = label;
    // Do not break group UID, linked patterns, visibility or user-controlled blending.
    scene.groups[group_index].shapes = new.shapes;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_pieces_three_styles_are_editable_serializable_and_bounded() {
        for piece in Piece::ALL {
            for design in Design::ALL {
                let opts = Settings { piece, design, ..Settings::default() };
                let g = make_piece(opts,[14.0,-22.0],"sample");
                assert_eq!(preset_of(&g),Some(opts));
                assert!(!g.shapes.is_empty() && g.shapes.len() < 25);
                assert_eq!(g.shapes[0].kind, Kind::Revolve);
                assert!(g.shapes.iter().all(|x| x.kind != Kind::Mesh && x.volume.is_none()));
                assert!(g.shapes.iter().all(|x| x.pos.iter().all(|v| v.is_finite())));
                assert!(g.shapes.iter().any(|x| x.op == Op::Add));
                assert!(g.shapes[0].outline.len() <= crate::model::MAX_OUTLINE);
                if piece == Piece::Knight {
                    assert!(g.shapes.iter().any(|x| x.kind == Kind::Extrude && x.rot[0] == 90.0));
                    assert!(g.shapes.iter().filter(|x| x.kind == Kind::Sphere).count() >= 4);
                }
                let mut sc = Scene { groups: vec![g], ..Scene::default() };
                let before = sc.clone();
                sc.sanitize();
                assert_eq!(sc, before, "sanitizer silently altered {piece:?}/{design:?}");
                assert_eq!(Scene::from_json(&sc.to_json()).unwrap(),sc);
            }
        }
    }

    #[test]
    fn full_board_has_correct_counts_teams_coordinates_and_bounds() {
        let s = Settings::default();
        let groups = full_set(s,[0.0,0.0]);
        assert_eq!(groups.len(),33);
        assert_eq!(groups[0].shapes.len(),33);
        let mut seen = std::collections::HashSet::new();
        let mut kinds = std::collections::HashMap::new();
        for g in groups.iter().skip(1) {
            let meta = preset_of(g).unwrap();
            *kinds.entry((meta.side.name(),meta.piece.key())).or_insert(0) += 1;
            let c = g.shapes[0].pos;
            let key = ((c[0]*100.0).round() as i32,(c[1]*100.0).round() as i32);
            assert!(seen.insert(key),"overlapping pieces");
            assert!(c[2] >= 0.0);
        }
        assert_eq!(seen.len(),32);
        for side in Side::ALL {
            for (piece,n) in [(Piece::King,1),(Piece::Queen,1),(Piece::Rook,2),(Piece::Bishop,2),(Piece::Knight,2),(Piece::Pawn,8)] {
                assert_eq!(kinds.get(&(side.name(),piece.key())),Some(&n));
            }
        }
        let mut sc = Scene { groups, ..Scene::default() };
        assert!(sc.shape_count() <= MAX_SHAPES);
        let old = sc.clone();
        sc.sanitize();
        assert_eq!(sc,old);
    }

    #[test]
    fn insert_and_rebuild_preserve_scene_and_group_selection() {
        let mut scene = Scene::default();
        let start = insert_set(&mut scene,Settings::default(),[0.0,0.0]).unwrap();
        assert_eq!(start,0);
        assert_eq!(scene.groups.len(),33);
        let original = scene.groups[1].shapes.clone();
        let new = Settings { design: Design::Organic, diameter_mm: 27.0, ..Settings::default() };
        rebuild_piece(&mut scene,1,new).unwrap();
        assert_ne!(scene.groups[1].shapes,original);
        assert_eq!(preset_of(&scene.groups[1]),Some(new));
        assert!(rebuild_piece(&mut scene,0,new).is_err());
        assert_eq!(Scene::from_json(&scene.to_json()).unwrap(),scene);
        let before = scene.clone();
        assert!(insert_set(&mut scene, Settings::default(),[0.0,0.0]).is_ok());
        assert!(scene.groups.len() > before.groups.len());
    }

    #[test]
    fn settings_clamp_bad_values() {
        let bad = Settings { height_mm:f32::NAN,diameter_mm:f32::INFINITY, ornament:f32::NEG_INFINITY,..Settings::default() };
        let s = bad.safe();
        assert!(s.height_mm.is_finite() && s.diameter_mm.is_finite() && s.ornament.is_finite());
        assert!(s.diameter_mm >= 14.0);
    }

    /// Run explicitly when validating export changes. Meshing 18 pieces takes time.
    #[test]
    #[ignore = "slow: real watertight output needs Rust mesher on CI/Windows"]
    fn printable_closed_mesh_for_all_pieces_and_styles() {
        use crate::export_job::{self, Options, Scope};
        use crate::export_mesh::Progress;
        for piece in Piece::ALL {
            for design in Design::ALL {
                let s = Settings { piece, design, ..Settings::default() };
                let scene = Scene { groups: vec![make_piece(s,[0.0,0.0],"test")], ..Scene::default() };
                let opts = Options { tolerance:0.45, scope: Scope::Visible, ..Options::default() };
                let built = export_job::build(&scene,None,&opts,&Progress::new()).unwrap();
                assert!(built.closed && !built.surface.mesh.triangles.is_empty(), "{piece:?}/{design:?}");
            }
        }
    }
}
