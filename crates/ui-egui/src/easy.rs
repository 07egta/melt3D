//! 「かんたん」タブ: Rhinoでよく使う操作（使う機能の7割を占める3割）を1つのタブにまとめる。
//!
//! 全コマンドはこれまで通り他のタブ・検索(F / S)・コマンド欄から呼べる。ここは「前に出す」だけ。
//! Rhinoのコマンド名（Sweep1, FilletEdge, BooleanUnion …）と日本語名でも、コマンド欄と検索から
//! 呼べるよう、別名表も持つ。

use solidcraft_engine::CommandSpec;

/// The tab's name in `toolbar::TABS`.
pub const TAB: &str = "EASY";

/// Panels, left to right: what people do, in the order they do it.
pub const PANELS: &[&str] = &["DRAW", "SHAPE", "SURFACE", "REFINE", "ARRANGE", "CHECK", "OUTPUT"];

const DRAW: &[&str] = &[
    "sketch.create",
    "sketch.line",
    "sketch.rectangle.two_point",
    "sketch.circle.center",
    "sketch.arc.three_point",
    "sketch.polygon.inscribed",
    "sketch.spline.fit_point",
    "sketch.spline.control_point",
    "sketch.ellipse",
    "sketch.text",
    "sketch.offset",
    "sketch.trim",
    "sketch.fillet",
    "sketch.mirror",
    "sketch.insert_svg",
    "sketch.insert_dxf",
];

/// The drawing tools that start a sketch on the XY plane by themselves when none is open
/// (Rhino draws straight away; no plane has to be chosen first).
const AUTO_SKETCH: &[&str] = &[
    "sketch.line",
    "sketch.rectangle.two_point",
    "sketch.rectangle.center",
    "sketch.circle.center",
    "sketch.circle.two_point",
    "sketch.circle.three_point",
    "sketch.arc.three_point",
    "sketch.arc.center_point",
    "sketch.polygon.inscribed",
    "sketch.ellipse",
    "sketch.spline.fit_point",
    "sketch.spline.control_point",
    "sketch.point",
];

const SHAPE: &[&str] = &[
    "solid.extrude",
    "solid.revolve",
    "solid.sweep",
    "solid.loft",
    "solid.pipe",
    "solid.box",
    "solid.cylinder",
    "solid.sphere",
    "solid.torus",
    "solid.coil",
    "solid.hole",
    "solid.rib",
    "solid.emboss",
];

const SURFACE: &[&str] = &[
    "surface.extrude",
    "surface.revolve",
    "surface.sweep",
    "surface.loft",
    "surface.ruled",
    "surface.patch",
    "surface.blend",
    "surface.stitch",
    "surface.trim",
    "surface.extend",
    "surface.offset",
    "surface.thicken",
    "surface.unstitch",
    "surface.untrim",
    "surface.reverse_normal",
];

const REFINE: &[&str] = &[
    "solid.fillet",
    "solid.chamfer",
    "solid.combine",
    "solid.shell",
    "solid.melt",
    "solid.press_pull",
    "solid.draft",
    "solid.offset_face",
    "solid.split_body",
    "solid.split_face",
    "solid.replace_face",
];

const ARRANGE: &[&str] =
    &["solid.move", "solid.mirror", "solid.scale", "solid.pattern.rectangular", "solid.pattern.circular", "solid.pattern.path", "solid.align"];

const CHECK: &[&str] = &[
    "inspect.measure",
    "inspect.continuity",
    "inspect.zebra",
    "inspect.curvature_map",
    "inspect.curvature_comb",
    "inspect.environment_map",
    "inspect.isocurve",
    "inspect.minimum_radius",
    "inspect.section",
    "inspect.draft",
];

const OUTPUT: &[&str] = &["file.save_mesh", "file.export", "file.save", "parameters.change", "file.insert_step", "file.insert_mesh"];

/// The commands of an easy panel in order and how many of them get a big button.
/// Commands that are only planned (registered, not built) are left out until they are built.
pub fn layout<'a>(panel: &str, specs: &[&'a CommandSpec]) -> Option<(Vec<&'a CommandSpec>, usize)> {
    let (ids, promoted): (&[&str], usize) = match panel {
        "DRAW" => (DRAW, 8),
        "SHAPE" => (SHAPE, 6),
        "SURFACE" => (SURFACE, 6),
        "REFINE" => (REFINE, 5),
        "ARRANGE" => (ARRANGE, 6),
        "CHECK" => (CHECK, 4),
        "OUTPUT" => (OUTPUT, 3),
        _ => return None,
    };
    let out = ids.iter().filter_map(|id| specs.iter().find(|c| c.id == *id).copied()).filter(|c| c.planned.is_none()).collect();
    Some((out, promoted))
}

/// A drawing tool that opens an XY sketch by itself when no sketch is open.
pub fn auto_sketch(id: &str) -> bool {
    AUTO_SKETCH.contains(&id)
}

/// Other names for commands: Rhino's command names, then Japanese ones. Matching ignores case.
const ALIASES: &[(&str, &[&str])] = &[
    // Curves
    ("sketch.line", &["Line", "Polyline", "Pline", "線", "直線", "ポリライン"]),
    ("sketch.rectangle.two_point", &["Rectangle", "Rect", "四角形", "長方形"]),
    ("sketch.rectangle.center", &["RectangleCenter", "中心から四角形"]),
    ("sketch.circle.center", &["Circle", "円"]),
    ("sketch.circle.three_point", &["Circle3Pt", "3点円"]),
    ("sketch.arc.three_point", &["Arc", "円弧", "3点円弧"]),
    ("sketch.polygon.inscribed", &["Polygon", "多角形", "ポリゴン"]),
    ("sketch.spline.control_point", &["Curve", "Crv", "制御点曲線", "曲線"]),
    ("sketch.spline.fit_point", &["InterpCrv", "InterpolateCrv", "Spline", "通過点曲線", "スプライン"]),
    ("sketch.ellipse", &["Ellipse", "楕円"]),
    ("sketch.text", &["Text", "TextObject", "文字", "テキスト"]),
    ("sketch.offset", &["Offset", "OffsetCrv", "オフセット"]),
    ("sketch.trim", &["Trim", "トリム", "切り取り"]),
    ("sketch.fillet", &["FilletCorner", "CurveFillet", "角を丸める"]),
    ("sketch.mirror", &["MirrorCrv", "線対称"]),
    ("sketch.project", &["Project", "Pull", "投影"]),
    ("sketch.point", &["Point", "点"]),
    ("sketch.line3d", &["Line3D", "3D線"]),
    ("sketch.spline3d", &["Curve3D", "3D曲線"]),
    ("sketch.insert_svg", &["ImportSVG", "SVG"]),
    ("sketch.insert_dxf", &["ImportDXF", "DXF"]),
    // Solids
    ("solid.extrude", &["Extrude", "ExtrudeCrv", "ExtrudeSrf", "押し出し", "押出し", "厚みをつける"]),
    ("solid.revolve", &["Revolve", "回転体", "回転"]),
    ("solid.sweep", &["Sweep1", "Sweep2", "Sweep", "スイープ", "レールに沿って"]),
    ("solid.loft", &["Loft", "ロフト"]),
    ("solid.pipe", &["Pipe", "パイプ", "管"]),
    ("solid.box", &["Box", "ボックス", "箱", "直方体"]),
    ("solid.cylinder", &["Cylinder", "円柱"]),
    ("solid.sphere", &["Sphere", "球"]),
    ("solid.torus", &["Torus", "トーラス", "ドーナツ"]),
    ("solid.coil", &["Helix", "Spiral", "Coil", "らせん", "螺旋", "コイル"]),
    ("solid.hole", &["Hole", "穴"]),
    ("solid.rib", &["Rib", "リブ"]),
    ("solid.emboss", &["Emboss", "Deboss", "エンボス", "刻印"]),
    // Editing
    ("solid.fillet", &["Fillet", "FilletEdge", "BlendEdge", "フィレット", "角丸", "丸める"]),
    ("solid.chamfer", &["Chamfer", "ChamferEdge", "面取り", "C面"]),
    (
        "solid.combine",
        &[
            "BooleanUnion",
            "BooleanDifference",
            "BooleanIntersection",
            "BooleanSplit",
            "Union",
            "Subtract",
            "Intersect",
            "ブーリアン",
            "結合",
            "合体",
            "差",
            "引き算",
            "交差",
        ],
    ),
    ("solid.shell", &["Shell", "ShellOpen", "シェル", "肉抜き", "中空"]),
    ("solid.melt", &["Melt", "SmoothUnion", "SoftUnion", "溶かす", "なめらか結合", "とろける"]),
    ("solid.press_pull", &["PushPull", "MoveFace", "面を押す", "面を引く"]),
    ("solid.draft", &["DraftAngle", "抜き勾配", "テーパー"]),
    ("solid.offset_face", &["OffsetFace", "面オフセット"]),
    ("solid.split_body", &["Split", "分割"]),
    ("solid.split_face", &["SplitFace", "面を分割"]),
    ("solid.replace_face", &["ReplaceFace", "面を置き換え"]),
    // Transform
    ("solid.move", &["Move", "Copy", "移動", "コピー", "複製"]),
    ("solid.mirror", &["Mirror", "ミラー", "鏡像", "左右反転"]),
    ("solid.scale", &["Scale", "Scale1D", "Scale2D", "拡大縮小", "スケール"]),
    ("solid.pattern.rectangular", &["Array", "ArrayLinear", "直線状配列", "配列", "並べる"]),
    ("solid.pattern.circular", &["ArrayPolar", "円形配列", "回転配列"]),
    ("solid.pattern.path", &["ArrayCrv", "曲線に沿って配列", "パス配列"]),
    ("solid.align", &["Align", "OrientOnSrf", "位置合わせ"]),
    // Surfaces
    ("surface.extrude", &["ExtrudeSurface", "面押し出し", "曲面押し出し"]),
    ("surface.revolve", &["RevolveSrf", "面回転"]),
    ("surface.sweep", &["SweepSrf", "Sweep1Srf", "Sweep2Srf", "面スイープ"]),
    ("surface.loft", &["LoftSrf", "面ロフト"]),
    ("surface.ruled", &["RuledSrf", "ルールド面"]),
    ("surface.patch", &["Patch", "NetworkSrf", "パッチ", "穴を埋める", "ネットワーク曲面"]),
    ("construct.plane.from_view", &["CPlane", "SetCPlane", "ViewPlane", "画面平面", "ビュー平面", "画面に平行"]),
    ("surface.blend", &["BlendSrf", "MatchSrf", "ブレンド曲面", "つなぐ曲面", "なじませる"]),
    ("surface.stitch", &["Join", "JoinSrf", "つなぐ", "縫合"]),
    ("surface.trim", &["TrimSrf", "面トリム"]),
    ("surface.extend", &["ExtendSrf", "面延長"]),
    ("surface.offset", &["OffsetSrf", "面オフセット曲面"]),
    ("surface.thicken", &["Thicken", "厚み付け", "ソリッド化"]),
    ("surface.unstitch", &["Explode", "Unjoin", "分解", "ばらす"]),
    ("surface.untrim", &["Untrim", "トリム解除"]),
    ("surface.reverse_normal", &["Flip", "Dir", "向きを反転", "法線反転"]),
    // Checking
    ("inspect.measure", &["Distance", "Length", "Area", "Measure", "計測", "距離", "長さ"]),
    ("inspect.continuity", &["GCon", "ContinuityAnalysis", "連続性", "G1G2G3", "つながり確認"]),
    ("inspect.zebra", &["Zebra", "ZebraAnalysis", "ゼブラ"]),
    ("inspect.curvature_map", &["CurvatureAnalysis", "曲率マップ", "曲率"]),
    ("inspect.curvature_comb", &["CurvatureGraph", "CurvatureComb", "曲率グラフ", "くし"]),
    ("inspect.environment_map", &["EMap", "EnvironmentMap", "環境マップ", "映り込み"]),
    ("inspect.isocurve", &["ExtractIsocurve", "Isocurve", "アイソカーブ"]),
    ("inspect.minimum_radius", &["RadiusAnalysis", "最小半径"]),
    ("inspect.section", &["Section", "Contour", "断面"]),
    ("inspect.draft", &["DraftAngleAnalysis", "勾配解析"]),
    // Files and editing
    ("file.save_mesh", &["ExportSTL", "STL", "3MF", "OBJ", "3Dプリント", "STL書き出し"]),
    ("file.export", &["Export", "ExportSTEP", "STEP", "IGES", "エクスポート", "書き出し"]),
    ("file.save", &["Save", "保存"]),
    ("file.new", &["New", "新規", "新しく作る"]),
    ("file.insert_step", &["ImportSTEP", "STEP読み込み"]),
    ("file.insert_mesh", &["ImportSTL", "STL読み込み"]),
    ("parameters.change", &["Parameters", "パラメータ", "寸法の表"]),
    ("edit.undo", &["Undo", "元に戻す"]),
    ("edit.redo", &["Redo", "やり直し"]),
];

/// How well `query` (ASCII-lowercased) names command `id` through its other names:
/// 0 = exactly, 1 = as the start of one, 2 = inside one, `None` = not at all.
pub fn alias_rank(id: &str, query: &str) -> Option<u8> {
    if query.is_empty() {
        return None;
    }
    let (_, names) = ALIASES.iter().find(|(i, _)| *i == id)?;
    let mut best: Option<u8> = None;
    for name in names.iter() {
        let name = name.to_ascii_lowercase();
        let rank = if name == query {
            0
        } else if name.starts_with(query) {
            1
        } else if name.contains(query) {
            2
        } else {
            continue;
        };
        best = Some(best.map_or(rank, |b| b.min(rank)));
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids_of(panel: &str) -> Vec<&'static str> {
        let specs = solidcraft_engine::command_specs();
        layout(panel, &specs).map(|(v, _)| v.iter().map(|c| c.id).collect()).unwrap_or_default()
    }

    #[test]
    fn every_panel_has_buttons_and_the_core_tools_are_up_front() {
        for panel in PANELS {
            assert!(!ids_of(panel).is_empty(), "{panel} is empty");
        }
        let shape = ids_of("SHAPE");
        assert_eq!(&shape[..4], &["solid.extrude", "solid.revolve", "solid.sweep", "solid.loft"]);
        assert_eq!(ids_of("REFINE").first().copied(), Some("solid.fillet"));
    }

    #[test]
    fn every_listed_and_aliased_command_exists() {
        let specs = solidcraft_engine::command_specs();
        let lists: [&[&str]; 8] = [DRAW, AUTO_SKETCH, SHAPE, SURFACE, REFINE, ARRANGE, CHECK, OUTPUT];
        for ids in lists {
            for id in ids {
                assert!(specs.iter().any(|c| c.id == *id) || is_future(id), "unknown command in the easy tab: {id}");
            }
        }
        for (id, names) in ALIASES {
            assert!(specs.iter().any(|c| c.id == *id) || is_future(id), "alias for an unknown command: {id}");
            assert!(!names.is_empty());
        }
    }

    /// Commands this tab lists before they exist (they are hidden until they are built).
    fn is_future(id: &str) -> bool {
        matches!(id, "solid.melt" | "surface.blend" | "inspect.continuity")
    }

    #[test]
    fn rhino_and_japanese_names_find_the_right_command() {
        let first = |q: &str| crate::palette::matches(q).first().map(|m| m.0);
        assert_eq!(first("Sweep1"), Some("solid.sweep"));
        assert_eq!(first("loft"), Some("solid.loft"));
        assert_eq!(first("FilletEdge"), Some("solid.fillet"));
        assert_eq!(first("fillet"), Some("solid.fillet"), "the solid fillet wins over the sketch corner");
        assert_eq!(first("BooleanUnion"), Some("solid.combine"));
        assert_eq!(first("ブーリアン"), Some("solid.combine"));
        assert_eq!(first("押し出し"), Some("solid.extrude"));
        assert_eq!(first("ArrayPolar"), Some("solid.pattern.circular"));
        assert_eq!(first("Zebra"), Some("inspect.zebra"));
        assert_eq!(first("ExportSTL"), Some("file.save_mesh"));
    }

    #[test]
    fn drawing_tools_open_a_sketch_by_themselves() {
        assert!(auto_sketch("sketch.line") && auto_sketch("sketch.circle.center"));
        assert!(!auto_sketch("sketch.trim") && !auto_sketch("solid.extrude"));
    }
}
