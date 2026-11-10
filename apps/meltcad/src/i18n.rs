//! UI text in each supported language. Japanese is the default table; English is complete.
//!
//! The settings branch owns persistence (`meltcad.preferences.v1`, field `lang`, default
//! `ja`). After loading the preferences and whenever `lang` changes, call
//! [`set_language`] on the UI thread:
//!
//! ```ignore
//! crate::i18n::set_language(&prefs.lang);
//! ```
//!
//! The language is kept per thread, so a background job can never change the text of the
//! window. Keys are stable strings grouped by area (`tool.*`, `inspector.*`, `scene.*`,
//! `status.*`, `prefs.*`, `dialog.*`, `error.*`). A missing key returns itself; a string
//! missing from the chosen language falls back to English, then Japanese. No value contains
//! a format placeholder: the caller composes numbers around the translated word.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The language codes the settings window offers; first is the default.
// The settings branch is the caller; the hook stays ready here.
#[allow(dead_code)]
pub const SUPPORTED: [&str; 7] = ["ja", "en", "zh-Hans", "ko", "de", "fr", "es"];

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Language {
    #[default]
    Ja,
    En,
    ZhHans,
    Ko,
    De,
    Fr,
    Es,
}

#[allow(dead_code)]
impl Language {
    fn code(self) -> &'static str {
        match self {
            Language::Ja => "ja",
            Language::En => "en",
            Language::ZhHans => "zh-Hans",
            Language::Ko => "ko",
            Language::De => "de",
            Language::Fr => "fr",
            Language::Es => "es",
        }
    }

    /// Fixed names, always written in their own language for the language picker.
    fn native_name(self) -> &'static str {
        match self {
            Language::Ja => "日本語",
            Language::En => "English",
            Language::ZhHans => "简体中文",
            Language::Ko => "한국어",
            Language::De => "Deutsch",
            Language::Fr => "Français",
            Language::Es => "Español",
        }
    }

    /// Accept the fixed codes and a few common spellings; anything else is not supported.
    fn parse(code: &str) -> Option<Language> {
        match code.trim().to_ascii_lowercase().as_str() {
            "ja" | "jp" | "japanese" => Some(Language::Ja),
            "en" | "english" => Some(Language::En),
            "zh-hans" | "zh" | "zh-cn" | "zh_cn" | "chinese" => Some(Language::ZhHans),
            "ko" | "kr" | "korean" => Some(Language::Ko),
            "de" | "german" => Some(Language::De),
            "fr" | "french" => Some(Language::Fr),
            "es" | "spanish" => Some(Language::Es),
            _ => None,
        }
    }
}

thread_local! {
    static CURRENT: std::cell::Cell<Language> = const { std::cell::Cell::new(Language::Ja) };
}

/// Choose the language shown by `tr` on this thread. Unknown codes use Japanese.
#[allow(dead_code)]
pub fn set_language(code: &str) {
    let language = Language::parse(code).unwrap_or_default();
    CURRENT.with(|current| current.set(language));
}

/// The code of the language shown by `tr` on this thread (`ja`, `en`, `zh-Hans`, ...).
#[allow(dead_code)]
pub fn language() -> &'static str {
    CURRENT.with(std::cell::Cell::get).code()
}

/// The name of a supported language in that language, for the language picker.
/// An unsupported code returns an empty string.
#[allow(dead_code)]
pub fn language_name(code: &str) -> &'static str {
    Language::parse(code).map(Language::native_name).unwrap_or("")
}

struct Entry {
    key: &'static str,
    ja: &'static str,
    en: &'static str,
    zh: &'static str,
    ko: &'static str,
    de: &'static str,
    fr: &'static str,
    es: &'static str,
}

/// An entry with only the mandatory Japanese and English columns.
macro_rules! e {
    ($key:literal, $ja:literal, $en:literal) => {
        Entry { key: $key, ja: $ja, en: $en, zh: "", ko: "", de: "", fr: "", es: "" }
    };
    ($key:literal, $ja:literal, $en:literal, $zh:literal, $ko:literal, $de:literal, $fr:literal, $es:literal) => {
        Entry { key: $key, ja: $ja, en: $en, zh: $zh, ko: $ko, de: $de, fr: $fr, es: $es }
    };
}

const TABLE: &[Entry] = &[
    // ----- window menu bar -----
    e!("menu.file", "ファイル", "File", "文件", "파일", "Datei", "Fichier", "Archivo"),
    e!("menu.edit", "編集", "Edit", "编辑", "편집", "Bearbeiten", "Édition", "Editar"),
    e!("menu.view", "表示", "View", "视图", "보기", "Ansicht", "Affichage", "Ver"),
    e!("menu.export", "書き出し", "Export", "导出", "내보내기", "Export", "Exporter", "Exportar"),
    e!("menu.find", "検索", "Find", "查找", "찾기", "Suchen", "Rechercher", "Buscar"),
    e!("menu.find.tip", "形、ライブラリ、コマンドを検索 (F)", "Find shapes, library, commands (F)"),
    e!("menu.menu", "メニュー", "Menu"),
    e!("menu.view.scene_dock", "シーン一覧", "Scene list"),
    e!(
        "menu.view.scene_dock.tip",
        "形の一覧とライブラリを出す・しまう。狭い画面では右の設定としかわりばんこに出ます",
        "Show or hide the shape list and library. A narrow window shows it or the settings, one at a time"
    ),
    e!("menu.view.inspector", "設定パネル", "Settings panel"),
    e!(
        "menu.view.inspector.tip",
        "選んだ形の大きさ・素材などを出す・しまう。しまうと3Dの画面が広くなります",
        "Show or hide the size, material and more of the selection. Hiding it makes the 3D view wider"
    ),
    e!("menu.view.simple", "かんたん表示", "Simple view"),
    e!(
        "menu.view.simple.tip",
        "あまり使わない設定を「くわしく」の中にたたみます。機能はなくなりません",
        "Fold less used settings under \"More\" headings. Nothing is removed"
    ),
    e!("menu.view.mouse", "マウスで操作", "Use a mouse"),
    e!("menu.view.trackpad", "トラックパッドで操作", "Use a trackpad"),
    // ----- tool strip: operations and tools -----
    e!("tool.op.add", "追加", "Add", "添加", "추가", "Hinzufügen", "Ajouter", "Añadir"),
    e!("tool.op.cut", "削除", "Cut", "切除", "자르기", "Schneiden", "Couper", "Cortar"),
    e!("tool.op.paint", "塗る", "Paint", "涂色", "칠하기", "Malen", "Peindre", "Pintar"),
    e!("tool.op.overlap", "重ねる", "Overlap", "重叠", "겹치기", "Überlappen", "Superposer", "Superponer"),
    e!(
        "tool.op.add.tip",
        "新しい形をモデルに加えます（Tabで追加と削除を切り替え）",
        "Join: the new shape is added to the model (Tab flips Add / Cut)"
    ),
    e!("tool.op.cut.tip", "新しい形でモデルに穴をあけます", "Cut: the new shape carves a hole out of the model"),
    e!("tool.op.paint.tip", "形は変えず、触れた場所の色と素材だけを変えます", "Paint: the new shape only changes the colour where it touches"),
    e!("tool.op.overlap.tip", "新しい形とモデルが重なる部分だけを残します", "Overlap: keep only the part where the new shape and the model meet"),
    e!("tool.group.new", "新しいグループ", "New group", "新建组", "새 그룹", "Neue Gruppe", "Nouveau groupe", "Grupo nuevo"),
    e!("tool.group.new.tip", "選択した形をまとめる新しいグループを作ります", "New group"),
    e!("tool.melt", "溶かす", "Melt", "融化", "녹이기", "Schmelzen", "Fondre", "Fundir"),
    e!(
        "tool.melt.tip",
        "選択した形を1クリックで溶かします（もう一度で元に戻る）。未選択ならシーン全体",
        "Melt the selected shapes together in one click (again to un-melt). Nothing selected: the whole scene"
    ),
    e!("tool.pattern", "パターン", "Pattern", "阵列", "패턴", "Muster", "Motif", "Patrón"),
    e!(
        "tool.pattern.tip",
        "選択したグループを繰り返します（ミラー、グリッド、円形、線に沿う）",
        "Repeat the selected group: mirror, grid, circle, along a path"
    ),
    e!("tool.more.title", "その他の形", "Other shapes"),
    e!("tool.melt_preset", "溶けた形（液だれ）…", "Melt (dripping shape)…"),
    e!(
        "tool.melt_preset.tip",
        "球や箱から重力で垂れる形を作ります。後から編集できます",
        "A gravity drip made from a sphere or box; editable afterwards"
    ),
    e!("tool.organic_blob", "有機ブロブ…", "Organic blob…"),
    e!("tool.organic_blob.tip", "種とスライダーから、なめらかな不規則な形を作ります", "A smooth irregular shape from a seed and sliders"),
    e!("tool.organic_tube", "有機チューブ", "Organic tube"),
    e!("tool.organic_tube.tip", "点、太さ、断面は後から編集できます", "Points, thickness and section stay editable afterwards"),
    e!("tool.organic_furniture", "有機家具", "Organic furniture"),
    e!("tool.organic_furniture.tip", "幅、高さ、脚は作った後も編集できます", "Width, height and legs stay editable after it is made"),
    e!("tool.cad", "CADツール", "CAD tools"),
    e!("tool.sketch", "スケッチ", "Sketch"),
    e!(
        "tool.sketch.tip",
        "スケッチを描きます。左クリックで点を置き、右クリックで完成。その後に立体の操作を選びます",
        "Draw a sketch. Left-click places points; right-click finishes. Choose a solid operation afterwards"
    ),
    e!("tool.solid", "立体", "Solid"),
    e!(
        "tool.solid.tip",
        "スケッチを描いて立体を作ります（SolidCraftのスケッチ画面）。上のボタンで戻ります",
        "Draw a sketch and make a solid from it (SolidCraft's sketch screen). Come back with the button at the top"
    ),
    e!("tool.revolve", "回転", "Revolve"),
    e!(
        "tool.revolve.tip",
        "立てた平面の完成したスケッチを選び、元の軸の周りに回転させます",
        "Select a finished sketch on a standing plane, then revolve it around its original axis"
    ),
    e!("tool.loft", "ロフト", "Loft"),
    e!("tool.loft.tip", "完成したスケッチを選び、上の輪郭までつなげます", "Select a finished sketch, then blend it into a top outline"),
    e!("tool.extrude", "押し出し", "Extrude"),
    e!(
        "tool.extrude.tip",
        "完成したスケッチを選び、深さを決めて右クリックで確定します",
        "Select a finished sketch, set its depth, then right-click to confirm"
    ),
    e!("tool.sweep", "スイープ", "Sweep"),
    e!(
        "tool.sweep.tip",
        "丸いチューブなら保存したパスを、輪郭のスイープなら閉じたスケッチを選びます",
        "Select a saved path for a round tube, or a closed sketch for a profile sweep"
    ),
    e!("tool.curve", "曲線", "Curve"),
    e!("tool.curve.tip", "曲線（クリックで点を置き、線を描きます）", "Curve (click to place nodes, draw a line)"),
    e!("tool.pipe", "パイプ", "Pipe"),
    e!("tool.pipe.tip", "パイプ（画面をドラッグしてパイプをなぞります）", "Pipe (drag across the view to trace a pipe)"),
    e!(
        "error.select_sketch",
        "先に完成したスケッチを選んでください。スケッチで描いて右クリックで完成します",
        "Select a finished sketch first. Use Sketch to draw one; right-click to finish it"
    ),
    e!(
        "tool.solidcraft.tip",
        "SolidCraft: 精密なスケッチと立体を作る。戻ると自動で Melt に取り込み、とけあう形にできます",
        "SolidCraft: precise sketches and solids. Going back imports them into Melt, ready to melt together"
    ),
    // ----- shape kinds (display only; the data keeps the English name) -----
    e!("kind.sphere", "球", "Sphere", "球体", "구", "Kugel", "Sphère", "Esfera"),
    e!("kind.box", "箱", "Box", "立方体", "상자", "Würfel", "Boîte", "Caja"),
    e!("kind.cylinder", "円柱", "Cylinder", "圆柱", "원기둥", "Zylinder", "Cylindre", "Cilindro"),
    e!("kind.torus", "ドーナツ", "Torus", "圆环", "토러스", "Torus", "Tore", "Toro"),
    e!("kind.cone", "円錐", "Cone", "圆锥", "원뿔", "Kegel", "Cône", "Cono"),
    e!("kind.extrude", "押し出し", "Extrude", "拉伸", "돌출", "Extrusion", "Extrusion", "Extrusión"),
    e!("kind.prism3", "角柱", "Prism", "棱柱", "각기둥", "Prisma", "Prisme", "Prisma"),
    e!("kind.prism_n", "多角柱", "Polygon Prism", "多边形棱柱", "다각기둥", "Vieleckprisma", "Prisme polygonal", "Prisma poligonal"),
    e!("kind.octa", "八面体", "Octahedron", "八面体", "팔면체", "Oktaeder", "Octaèdre", "Octaedro"),
    e!("kind.tetra", "四面体", "Tetrahedron", "四面体", "사면체", "Tetraeder", "Tétraèdre", "Tetraedro"),
    e!("kind.pyramid", "ピラミッド", "Pyramid", "金字塔", "피라미드", "Pyramide", "Pyramide", "Pirámide"),
    e!("kind.capsule", "カプセル", "Capsule", "胶囊", "캡슐", "Kapsel", "Capsule", "Cápsula"),
    e!("kind.hemi", "半球", "Hemisphere", "半球", "반구", "Halbkugel", "Hémisphère", "Hemisferio"),
    e!("kind.tube", "パイプ", "Pipe", "管", "파이프", "Rohr", "Tuyau", "Tubo"),
    e!("kind.wedge", "くさび", "Wedge", "楔", "쐐기", "Keil", "Coin", "Cuña"),
    e!("kind.star", "星型", "Star Prism", "星形棱柱", "별기둥", "Sternprisma", "Prisme étoilé", "Prisma estrellado"),
    e!("kind.revolve", "回転体", "Revolve", "旋转体", "회전체", "Rotationskörper", "Volume de révolution", "Cuerpo de revolución"),
    e!("kind.loft", "ロフト", "Loft", "放样", "로프트", "Loft", "Lissage", "Loft"),
    e!("kind.curve", "曲線", "Curve", "曲线", "곡선", "Kurve", "Courbe", "Curva"),
    e!("kind.sweep", "スイープ", "Sweep", "扫掠", "스윕", "Sweep", "Balayage", "Barrido"),
    e!("kind.mesh", "メッシュ", "Mesh", "网格", "메시", "Mesh", "Maillage", "Malla"),
    // ----- inspector tabs and titles -----
    e!("inspector.tab.shape", "形", "Shape", "形状", "셰이프", "Form", "Forme", "Forma"),
    e!("inspector.tab.material", "素材", "Material", "材质", "재질", "Material", "Matière", "Material"),
    e!("inspector.tab.melt", "溶かす", "Melt", "融化", "녹이기", "Schmelzen", "Fondre", "Fundir"),
    e!("inspector.tab.light", "光", "Light", "光照", "조명", "Licht", "Lumière", "Luz"),
    e!("inspector.title.selection", "選択", "Selection", "选择", "선택", "Auswahl", "Sélection", "Selección"),
    e!("inspector.empty", "ビューかシーン一覧で形をクリックしてください", "Click a shape in the view or the scene list"),
    e!(
        "inspector.locked",
        "実行中のコマンドカードを終えてから、他の形を編集してください",
        "Use the active command card. Finish or cancel to edit other shapes."
    ),
    e!("inspector.title.shape", "形", "Shape", "形状", "셰이프", "Form", "Forme", "Forma"),
    e!("inspector.more.kind", "くわしく: 別の形に入れかえる", "More: change to another kind"),
    e!("inspector.more.details", "くわしく: 数値・丸め・溶かす・中抜き", "More: numbers, rounding, melt, shell"),
    e!("inspector.more.order", "くわしく: 並べかえ・グループへ移す", "More: reorder, move to a group"),
    e!("inspector.more.material", "くわしく: 金属・透明・光る・グラデーション", "More: metal, glass, glow, gradient"),
    e!("inspector.more.room", "くわしく: 影・空・ライト", "More: shadow, sky, lights"),
    e!("inspector.title.group", "グループ", "Group", "组", "그룹", "Gruppe", "Groupe", "Grupo"),
    e!("inspector.title.material", "素材", "Material", "材质", "재질", "Material", "Matière", "Material"),
    e!("inspector.title.sketch", "スケッチ", "Sketch", "草图", "스케치", "Skizze", "Esquisse", "Boceto"),
    e!("inspector.selected", "個選択（Shift+クリックで追加または解除）", "selected (Shift + click adds or removes)"),
    e!("inspector.group_them", "グループ化 (Ctrl+G)", "Group them (Ctrl+G)"),
    e!(
        "inspector.group_tip",
        "選択した形やグループを新しい1つのグループに入れます（グループは入れ子にできます）",
        "Put the selected shapes or groups inside one new group (groups can nest)"
    ),
    e!("inspector.op.add", "＋ 追加", "＋ Add", "＋ 添加", "＋ 추가", "＋ Hinzufügen", "＋ Ajouter", "＋ Añadir"),
    e!("inspector.op.cut", "− 削除", "− Subtract", "− 切除", "− 자르기", "− Schneiden", "− Couper", "− Cortar"),
    e!("inspector.op.paint", "● 塗る", "● Paint", "● 涂色", "● 칠하기", "● Malen", "● Peindre", "● Pintar"),
    e!("inspector.op.overlap", "◐ 重なり", "◐ Overlap", "◐ 重叠", "◐ 겹치기", "◐ Überlappen", "◐ Superposer", "◐ Superponer"),
    e!("inspector.op.add.tip", "この形を他の形とつなげます", "Join this shape with the others"),
    e!("inspector.op.cut.tip", "この形で穴をあけます", "Cut a hole with this shape"),
    e!("inspector.op.paint.tip", "形は変えず、色と素材だけを変えます", "Change only the color and material, not the shape"),
    e!("inspector.op.overlap.tip", "前の形と重なる部分だけを残します", "Keep only the part that overlaps the shape before"),
    e!("inspector.duplicate", "複製 (Ctrl+D)", "Duplicate (Ctrl+D)"),
    e!("inspector.delete", "削除 (Del)", "Delete (Del)"),
    e!("inspector.mirror", "ミラー複製", "Mirror copy", "镜像复制", "거울 복사", "Spiegeln", "Copie miroir", "Copia espejo"),
    e!("inspector.mirror.tip", "反対側にコピーします（耳、目、腕）", "Copy to the opposite side (ears, eyes, arms)"),
    e!("inspector.kind", "種類", "Kind", "种类", "종류", "Art", "Type", "Tipo"),
    e!("inspector.melt_on", "溶かす: オン", "Melt: on", "融化: 开", "녹이기: 켬", "Schmelzen: an", "Fondre : oui", "Fundir: sí"),
    e!("inspector.melt_off", "溶かす: オフ", "Melt: off", "融化: 关", "녹이기: 끔", "Schmelzen: aus", "Fondre : non", "Fundir: no"),
    e!(
        "inspector.melt.shape.tip",
        "オフ: この形は鋭いまま。オン: グループ内の他の形と溶けます",
        "Off: this shape stays sharp. On: it melts into the other shapes of its group"
    ),
    e!(
        "inspector.melt.group.tip",
        "オフ: このグループは鋭く独立したまま。オン: 隣の形と溶けます",
        "Off: this group stays a sharp, separate shape. On: it melts into what is next to it"
    ),
    e!("inspector.melt_amount", "溶け具合", "Melt amount", "融化量", "녹는 정도", "Schmelzstärke", "Intensité", "Cantidad de fusión"),
    e!(
        "inspector.melt.amount.shape.tip",
        "この形がグループ内の他の形とどれだけ溶けるか",
        "How much this shape melts into the other shapes of its group"
    ),
    e!(
        "inspector.melt.amount.group.tip",
        "このグループ全体が前のグループとどれだけ溶けるか",
        "How much this whole group melts into the groups before it"
    ),
    e!("inspector.round", "丸め", "Round", "圆角", "둥글게", "Runden", "Arrondi", "Redondeo"),
    e!("inspector.round.tip", "角と辺の丸み（mm）。形が壊れる手前で止まります", "Corner and edge radius (mm). Stops before the shape breaks"),
    e!("inspector.fillet", "フィレット", "Fillet", "圆角", "필렛", "Rund", "Congé", "Redondeo"),
    e!("inspector.fillet.tip", "角を丸くします", "Round off the corners"),
    e!("inspector.chamfer", "面取り", "Chamfer", "倒角", "모따기", "Fase", "Chanfrein", "Chaflán"),
    e!("inspector.chamfer.tip", "角を斜めに平らに切ります", "Cut the corners flat at an angle"),
    e!("inspector.shell", "中抜き", "Shell", "抽壳", "속 비우기", "Aushöhlen", "Évider", "Vaciar"),
    e!(
        "inspector.shell.tip",
        "内側をくり抜き、指定した厚さの壁を残します（mm）。0 = 中身が詰まったまま。穴は開かず、中が空洞になります（「引く」で削ると穴が開いてコップになります）",
        "Hollow out the inside, leaving walls this thick (mm). 0 = solid. The inside becomes a closed cavity; cut it with a Subtract to open it into a cup"
    ),
    e!(
        "inspector.melt_face",
        "1面だけ溶かす",
        "Melt one face",
        "只融化一个面",
        "한 면만 녹이기",
        "Eine Fläche schmelzen",
        "Fondre une face",
        "Fundir una cara"
    ),
    e!("inspector.face.none", "なし", "None", "无", "없음", "Keine", "Aucune", "Ninguna"),
    e!("inspector.face.none.tip", "形全体の大きさは変えません", "Whole shape keeps its exact size"),
    e!("inspector.twist.title", "Twist（ひねり）", "Twist"),
    e!("inspector.twist.hint", "元の形を残したまま、断面を滑らかに回転", "Turns each cross-section smoothly; the original shape stays editable"),
    e!("inspector.twist.off", "解除", "Off"),
    e!("inspector.twist.axis", "中心軸", "Axis"),
    e!("inspector.twist.direction", "方向", "Direction"),
    e!("inspector.twist.forward", "＋方向", "Forward"),
    e!("inspector.twist.reverse", "逆方向", "Reverse"),
    e!("inspector.twist.angle", "角度", "Angle"),
    e!("inspector.twist.angle.tip", "元の形の軸に沿った回転量 (°)", "How far the shape turns along its own axis (degrees)"),
    e!("inspector.twist.begin", "開始位置", "Start"),
    e!("inspector.twist.begin.tip", "元形状の軸の下端=0、上端=1", "Along the original axis: bottom = 0, top = 1"),
    e!("inspector.twist.end", "終了位置", "End"),
    e!("inspector.twist.end.tip", "この位置でねじり角度の変化が終了", "Where the turning stops changing"),
    e!("inspector.twist.lower", "下側の量", "Bottom amount"),
    e!("inspector.twist.lower.tip", "角度に対する下側の倍率 (0 は回転なし)", "Share of the angle at the bottom (0 = no turn)"),
    e!("inspector.twist.upper", "上側の量", "Top amount"),
    e!("inspector.twist.upper.tip", "角度に対する上側の倍率 (1 は指定角度)", "Share of the angle at the top (1 = the full angle)"),
    e!(
        "inspector.twist.round_note",
        "真円柱を中心軸でねじっても外形は同じです。角柱や箱で効果を確認できます。",
        "A round cylinder twisted about its own axis looks the same; try a box or a prism"
    ),
    e!(
        "inspector.twist.preview_note",
        "ドラッグ中は軽量表示、停止後は高品質表示。保存や3D書き出しは元データから計算します。",
        "A light preview while you drag; saving and 3D export are computed from the original shape"
    ),
    e!("inspector.swell", "ふくらみ", "Swell", "鼓起", "부풀림", "Wölbung", "Renflement", "Hinchazón"),
    e!(
        "inspector.swell.tip",
        "この面が外へ出る量（+）か内へ沈む量（-）、mm。他の面は大きさを保ちます",
        "How far this face swells out (+) or sinks in (-), in mm; other faces keep their size"
    ),
    e!("inspector.depth", "深さ", "Depth", "深度", "깊이", "Tiefe", "Profondeur", "Profundidad"),
    e!("inspector.depth.tip", "面からどれだけ溶かし込むか（mm）", "How far the melt reaches from the face, in mm"),
    e!("inspector.position", "位置", "Position", "位置", "위치", "Position", "Position", "Posición"),
    e!("inspector.rotation", "回転", "Rotation", "旋转", "회전", "Drehung", "Rotation", "Rotación"),
    e!("inspector.scale", "大きさ", "Scale", "缩放", "크기", "Größe", "Échelle", "Escala"),
    e!("inspector.guide.visible", "表示", "Visible"),
    e!("inspector.guide.closed", "閉じた輪郭", "Closed profile"),
    e!("inspector.guide.open", "開いた線", "Open path"),
    e!("inspector.guide.create", "立体を作る", "Create solid"),
    e!("inspector.guide.edit", "スケッチを編集", "Edit Sketch"),
    e!(
        "inspector.guide.edit.tip",
        "寸法を編集。右クリックでスケッチを保存、Escで下書きを取り消します",
        "Edit dimensions; right-click saves the sketch, Esc cancels the draft"
    ),
    e!("inspector.guide.path_points", "パス点を編集", "Edit path points"),
    e!("inspector.move_up", "上へ", "Move up one"),
    e!("inspector.move_down", "下へ", "Move down one"),
    e!("inspector.move_to_group", "グループへ移動", "Move to group"),
    e!("inspector.group.move_up", "上へ移動", "Move up"),
    e!("inspector.group.move_down", "下へ移動", "Move down"),
    e!("inspector.group.into_above", "→ 上のグループへ", "→ Into group above"),
    e!(
        "inspector.group.into_above.tip",
        "このグループを上のグループの中に入れ、1つとして溶かします",
        "Put this group inside the group above it, so they melt as one"
    ),
    e!("inspector.group.out", "← 外へ", "← Out"),
    e!("inspector.group.out.tip", "このグループを親グループの外に出します", "Take this group out of its parent group"),
    e!("inspector.group.wrap", "新しいグループで包む", "Wrap in new group"),
    e!("inspector.group.wrap.tip", "このグループの周りに新しい親グループを作ります", "Make a new parent group around this one"),
    e!("inspector.group.the_scene", "シーン全体", "the scene"),
    e!("inspector.group.label", "グループ", "group"),
    e!("inspector.group.melts_with", "溶け合う相手: ", "Melts with: "),
    e!("inspector.group.nothing", "なし", "nothing else"),
    e!("inspector.group.inside_open", "（", " (inside "),
    e!("inspector.group.inside_close", " の中）", ")"),
    e!("inspector.group.shapes_open", "「", "the shapes of “"),
    e!("inspector.group.shapes_close", "」の形", "”"),
    e!("inspector.group.linked_hint", "リンク複製: 上の位置・回転・大きさで設定します", "Linked copy: set position, rotation and scale above"),
    e!("inspector.group.move_hint", "グループ全体を移動・回転・拡大縮小", "Move, rotate and scale the whole group"),
    e!("inspector.pattern.title", "パターン", "Pattern", "阵列", "패턴", "Muster", "Motif", "Patrón"),
    e!("inspector.pattern.none", "パターンなし", "No pattern", "无阵列", "패턴 없음", "Kein Muster", "Aucun motif", "Sin patrón"),
    e!("inspector.pattern.empty_hint", "グループを何度も繰り返すか、左右にミラーします", "Repeat a group many times, or mirror it left and right"),
    e!("inspector.pattern.mirror", "ミラー", "Mirror", "镜像", "거울", "Spiegeln", "Miroir", "Espejo"),
    e!("inspector.pattern.mirror_plane", "ミラー面", "Mirror plane"),
    e!("inspector.pattern.origin", "原点", "Origin"),
    e!("inspector.pattern.group_center", "グループ中心", "Group center"),
    e!("inspector.pattern.count", "個数", "Count", "数量", "개수", "Anzahl", "Nombre", "Cantidad"),
    e!("inspector.pattern.spacing", "間隔", "Spacing", "间距", "간격", "Abstand", "Espacement", "Separación"),
    e!("inspector.pattern.angle", "角度", "Angle", "角度", "각도", "Winkel", "Angle", "Ángulo"),
    e!("inspector.pattern.rotation_axis", "回転軸", "Rotation axis"),
    e!("inspector.pattern.center", "中心", "Center"),
    e!("inspector.pattern.rotate_copies", "コピーも回転", "Rotate copies too"),
    e!("inspector.melt_preset.edit", "Meltの形を再編集…", "Edit this melt preset…"),
    e!(
        "inspector.melt_preset.edit_tip",
        "本数や垂れ方を変更。手動で動かした液だれは再生成されます",
        "Change the count and sag; drips you moved by hand are made again"
    ),
    e!("inspector.part.chair", "Designer Chair の個別パーツ", "Designer Chair part"),
    e!("inspector.part.chair_settings", "椅子全体の設定", "Chair settings"),
    e!("inspector.part.organic_open", "有機家具の設定を開く", "Open the organic furniture settings"),
    e!(
        "inspector.part.organic_note",
        "このパーツは家具プリセットの一部です。設定を変更すると配置を作り直します。",
        "This part belongs to a furniture preset. Changing the settings rebuilds the layout."
    ),
    e!("inspector.fusion.title", "Smooth Fusion（なめらか合体）", "Smooth Fusion"),
    e!("inspector.fusion.count", "個の形が別々に編集できます", "shapes can be edited separately"),
    e!("inspector.fusion.radius", "接合部の丸み", "Joint roundness"),
    e!(
        "inspector.fusion.radius.tip",
        "Smooth Unionの丸み（mm）。0mmは普通のBoolean Unionです",
        "Smooth Union roundness (mm). 0 mm is a plain Boolean union"
    ),
    e!("inspector.fusion.contact", "融合範囲", "Contact"),
    e!(
        "inspector.fusion.contact.tip",
        "0=深く重ねて完全融合、1=離して軽い接触。位置のみ更新、各形の回転とサイズは維持",
        "0 = overlap deeply and merge fully, 1 = stay apart with light contact. Only positions change; rotation and size stay"
    ),
    e!("inspector.fusion.full", "完全融合", "Fully merged"),
    e!("inspector.fusion.light", "軽い接触", "Light contact"),
    e!(
        "inspector.fusion.hint",
        "画面で形を直接ドラッグできます。形を個別に選ぶと位置・回転・大きさ・色を編集できます。",
        "Drag shapes directly in the view. Select one shape to edit its position, rotation, size and color."
    ),
    // ----- pattern kinds (the menu and the inspector) -----
    e!("pattern.mirror", "ミラー", "Mirror", "镜像", "거울", "Spiegeln", "Miroir", "Espejo"),
    e!("pattern.rect", "グリッド", "Rectangle", "矩形阵列", "직사각형", "Rechteck", "Rectangle", "Rectángulo"),
    e!("pattern.circular", "円形", "Circular", "圆形阵列", "원형", "Kreis", "Circulaire", "Circular"),
    e!("pattern.path", "線に沿う", "Along path", "沿路径", "경로 따라", "Entlang Pfad", "Le long du chemin", "A lo largo"),
    e!("pattern.geometry", "ジオメトリ", "Geometry", "几何", "지오메트리", "Geometrie", "Géométrie", "Geometría"),
    e!("pattern.mirror.hint", "X、Y、Z面でミラーします", "Mirrors across the X, Y or Z plane"),
    e!("pattern.rect.hint", "長さ、幅、高さの方向に均等に繰り返します", "Repeats evenly along length, width and height"),
    e!("pattern.circular.hint", "軸の周りに円形に繰り返します", "Repeats in a circle around an axis"),
    e!("pattern.path.hint", "線に沿って繰り返します（曲線、スイープ、形）", "Repeats along a line (curve, sweep or shape)"),
    e!("pattern.geometry.hint", "別の形の各点（節点や角）に1つずつコピーを置きます", "Puts one copy at each point (node or corner) of another shape"),
    // ----- scene list -----
    e!("scene.tab.scene", "シーン", "Scene", "场景", "장면", "Szene", "Scène", "Escena"),
    e!("scene.tab.library", "ライブラリ", "Library", "素材库", "라이브러리", "Bibliothek", "Bibliothèque", "Biblioteca"),
    e!("scene.shapes", "個の形", "shapes", "个形状", "개 형상", "Formen", "formes", "formas"),
    e!("scene.group", "+ グループ", "+ Group", "+ 组", "+ 그룹", "+ Gruppe", "+ Groupe", "+ Grupo"),
    e!("scene.group_them", "グループ化", "Group them", "合并成组", "그룹으로 묶기", "Gruppieren", "Grouper", "Agrupar"),
    e!(
        "scene.group_tip",
        "何も選ばないか1つだけ選ぶ: 新しい空のグループ。複数選択（Shift+クリック）: すべてを1つの大きなグループに入れる（Ctrl+G）。グループを別のグループの中央へドラッグしても入れ子にできます",
        "Nothing or one picked: a new empty group.\nSeveral picked (Shift+click): put them all in one new, bigger group (Ctrl+G).\nOr drag a group onto the middle of another group to put it inside"
    ),
    e!("scene.linked", "リンク", "Linked", "链接", "링크", "Verknüpft", "Lié", "Vinculado"),
    e!("scene.fusion", "融合", "Fusion", "融合", "융합", "Fusion", "Fusion", "Fusión"),
    e!("scene.file", "ファイル", "File", "文件", "파일", "Datei", "Fichier", "Archivo"),
    e!("scene.add_hint", "上の形をクリックして追加", "Click a shape above to add one"),
    e!("scene.locked", "並べ替える前に、実行中のコマンドを終了するか取り消してください", "Finish or cancel the active command before reordering"),
    // ----- status bar -----
    e!("status.animation", "アニメーション", "Animation", "动画", "애니메이션", "Animation", "Animation", "Animación"),
    e!("status.animation.tip", "タイムライン: ポーズ、再生、時間で溶かす、GIF書き出し", "Timeline: pose, play, melt over time, export a GIF"),
    e!("status.untitled", "無題", "Untitled", "未命名", "제목 없음", "Unbenannt", "Sans titre", "Sin título"),
    e!("status.shapes", "形", "Shapes", "形状", "형상", "Formen", "Formes", "Formas"),
    // The status bar keeps the compact axis letters in Japanese so an existing size test and
    // the familiar W × D × H reading stay unchanged.
    e!("status.width", "W", "W", "宽", "너비", "B", "L", "An"),
    e!("status.depth", "D", "D", "深", "깊이", "T", "P", "Pr"),
    e!("status.height", "H", "H", "高", "높이", "H", "H", "Al"),
    e!("status.about", "約", "about", "约", "약", "etwa", "environ", "aprox."),
    e!("status.group", "グループ", "Group", "组", "그룹", "Gruppe", "Groupe", "Grupo"),
    e!("status.hint.select", "クリックで選択", "Click to select"),
    e!(
        "status.hint.simple",
        "左のボタンで形を足す → 形をクリック → 右で大きさや色を変える",
        "Add a shape with the left buttons → click it → change its size and color on the right"
    ),
    e!("status.hint.all", "矢印で移動 · 曲線で回転 · 箱で引き伸ばし", "Drag an arrow to move · a curve to turn · a box to stretch"),
    e!("status.hint.move", "矢印をドラッグして移動（Ctrl: 1mm刻み）", "Drag arrow to move (Ctrl: 1 mm steps)"),
    e!("status.hint.rotate", "リングをドラッグして回転（Ctrl: 15度刻み）", "Drag ring to rotate (Ctrl: 15 degree steps)"),
    e!("status.hint.scale", "中央で全体を拡大 · 箱で一方向に引き伸ばし", "Drag the middle to grow all · a box to stretch one way"),
    // ----- first-use guide and status hints -----
    e!("guide.begin", "箱を1つ作ってみる", "Make one box"),
    e!("guide.close.tip", "案内を閉じる。次回は自動表示しません", "Close the guide. It will not open by itself again"),
    e!("guide.done.actions", "ガイド内の操作数", "Actions in the guide"),
    e!("guide.done.close", "閉じる", "Close"),
    e!(
        "guide.done.note",
        "このガイドは自動では再表示しません。F1 の操作一覧からいつでも開けます。",
        "This guide will not open by itself again. Open it any time from the F1 controls card."
    ),
    e!("guide.done.seconds", "開始からの秒数", "Seconds since the start"),
    e!("guide.done.title", "完成！　箱を1つ作れました", "Done! You made a box"),
    e!(
        "guide.extrude.body",
        "高さの矢印をドラッグするか、カードで高さを入れて「Make Extrude」か右クリックで確定します。",
        "Drag the height arrow or type a height in the card, then press Make Extrude or right-click."
    ),
    e!("guide.extrude.note", "高さは20mmのままで大丈夫。", "20 mm is fine as it is."),
    e!("guide.extrude.title", "押し出して箱にする", "Extrude it into a box"),
    e!("guide.hint.all", "矢印・リング・箱から操作を選ぶ", "Pick an arrow, a ring or a box to edit"),
    e!("guide.hint.all.keys", "左ドラッグ: 形を編集 / ホイール: ズーム", "Left drag: edit the shape / Wheel: zoom"),
    e!("guide.hint.circle", "中心からドラッグして円を描く", "Drag from the centre to draw a circle"),
    e!("guide.hint.curve", "3D画面で線の節点を置く", "Place the line's nodes in the 3D view"),
    e!("guide.hint.curve.keys", "左クリック: 点を追加 / Enter: 確定 / Esc: 終了", "Left-click: add point / Enter: finish / Esc: stop"),
    e!("guide.hint.drag.keys", "左ドラッグ または2回クリック / Esc: 取消", "Left drag or click twice / Esc: cancel"),
    e!("guide.hint.edit", "輪かくの点をドラッグして編集", "Drag the outline's points to edit it"),
    e!("guide.hint.edit.keys", "線をクリック: 点を追加 / Delete: 点を削除 / Esc: 終了", "Click a line: add point / Delete: remove point / Esc: done"),
    e!("guide.hint.empty", "ツールの「スケッチ」から形を描こう", "Start with Sketch in the tools"),
    e!("guide.hint.empty.keys", "左クリック: 選ぶ / 中ドラッグ: 視点を回す / F1: 操作一覧", "Left-click: select / Middle drag: orbit / F1: controls"),
    e!("guide.hint.hole", "輪かくの内側に穴を描く", "Draw a hole inside the outline"),
    e!("guide.hint.hole.keys", "左クリックかドラッグ / 右クリック: 確定 / Esc: 戻る", "Left-click or drag / Right-click: finish / Esc: back"),
    e!("guide.hint.line", "線の点を順番にクリック", "Click the line's points in order"),
    e!("guide.hint.line.keys", "Enter か右クリック: 線を確定 / Esc: 取消", "Enter or right-click: finish the line / Esc: cancel"),
    e!("guide.hint.live", "高さの矢印をドラッグして形を決める", "Drag the height arrow to shape the solid"),
    e!("guide.hint.live.keys", "右クリックか作るボタン: 確定 / Esc: 取消", "Right-click or the Make button: confirm / Esc: cancel"),
    e!("guide.hint.loft", "ロフトの上側の輪かくを描く", "Draw the loft's top outline"),
    e!("guide.hint.loft.keys", "左ドラッグかクリック / Enter: 確定 / Esc: 戻る", "Left drag or click / Enter: finish / Esc: back"),
    e!("guide.hint.move", "矢印をドラッグして形を動かす", "Drag an arrow to move the shape"),
    e!("guide.hint.move.keys", "W: 移動 / Ctrl: 1mm刻み / Ctrl+Z: 元に戻す", "W: move / Ctrl: 1 mm steps / Ctrl+Z: undo"),
    e!("guide.hint.pick", "形の面か地面をクリックして描く面を決める", "Click a face or the ground to choose where to draw"),
    e!("guide.hint.pick.keys", "左クリック: 面を選んで描き始める / Esc: 終了", "Left-click: choose the plane and start / Esc: stop"),
    e!("guide.hint.pipe", "3D画面をなぞってパイプを作る", "Trace in the 3D view to make a pipe"),
    e!("guide.hint.pipe.keys", "左ドラッグ: 描く / Esc: 終了", "Left drag: draw / Esc: stop"),
    e!(
        "guide.hint.points.keys",
        "最初の点・右クリック・Enter: 閉じる / Backspace: 1点戻す",
        "First point, right-click or Enter: close / Backspace: undo a point"
    ),
    e!("guide.hint.polygon", "輪かくの角を順番にクリック", "Click the outline's corners in order"),
    e!("guide.hint.ready.extrude", "輪かくができました。高さを決めて押し出す", "Outline done. Set the height and extrude"),
    e!("guide.hint.ready.keep", "スケッチができました。保存してから立体にする", "Sketch done. Save it, then make a solid from it"),
    e!(
        "guide.hint.ready.keep.keys",
        "右クリック: スケッチを保存 → 選んで押し出し / Esc: 描き直す",
        "Right-click: save the sketch, select it, then Extrude / Esc: redraw"
    ),
    e!("guide.hint.ready.keys", "作るボタンか右クリック: 確定 / Esc: 描き直す", "The Make button or right-click: confirm / Esc: redraw"),
    e!("guide.hint.ready.loft", "上下の輪かくをつなぐ", "Join the bottom and top outlines"),
    e!("guide.hint.ready.revolve", "輪かくを回転体にする", "Turn the outline into a revolved solid"),
    e!("guide.hint.ready.sweep", "線に沿ってスイープする", "Sweep the outline along the path"),
    e!("guide.hint.rect", "作業面でドラッグして四角を描く", "Drag on the plane to draw a rectangle"),
    e!("guide.hint.rotate", "リングをドラッグして回す", "Drag a ring to rotate"),
    e!("guide.hint.rotate.keys", "E: 回転 / Ctrl: 15度刻み / Ctrl+Z: 元に戻す", "E: rotate / Ctrl: 15 degree steps / Ctrl+Z: undo"),
    e!("guide.hint.scale", "ハンドルをドラッグして大きさを変える", "Drag a handle to resize"),
    e!("guide.hint.scale.keys", "R: 大きさ / Ctrl+Z: 元に戻す", "R: scale / Ctrl+Z: undo"),
    e!("guide.hint.select", "形をクリックして選ぶ", "Click a shape to select it"),
    e!(
        "guide.hint.select.keys",
        "左クリック: 選択 / 中ドラッグ: 視点を回す / ホイール: ズーム",
        "Left-click: select / Middle drag: orbit / Wheel: zoom"
    ),
    e!("guide.hint.spline", "制御点を順番にクリック", "Click the control points in order"),
    e!("guide.hint.sweep", "スイープの通り道を2点以上描く", "Draw a sweep path with at least 2 points"),
    e!("guide.hint.sweep.keys", "左クリック: 点 / Enter: 確定 / Esc: 戻る", "Left-click: point / Enter: finish / Esc: back"),
    e!("guide.intro", "新規作成 → 四角を描く → 押し出す。実際に箱を1つ作ります。", "New design, draw a rectangle, extrude it: you make a real box."),
    e!(
        "guide.intro.safe",
        "練習を始めても、いまの作品は自動では消えません。",
        "Starting the practice never throws away your current design by itself."
    ),
    e!("guide.make.extrude", "高さを決めたら押し出しを確定できます", "Confirm the extrusion once the height is right"),
    e!("guide.make.loft", "下側と上側の輪かくをつなぎます", "Joins the bottom and top outlines"),
    e!("guide.make.revolve", "輪かくを軸のまわりに回転させます", "Turns the outline around the axis"),
    e!("guide.make.sweep", "描いた通り道に沿って押し出します", "Pushes the outline along the drawn path"),
    e!(
        "guide.new.body",
        "新しい空の作品を用意します。未保存の変更があれば保存を確認します。",
        "Starts a new empty design. Unsaved changes are offered for saving first."
    ),
    e!("guide.new.title", "練習用の新規作成", "New practice design"),
    e!("guide.open", "箱づくりガイド", "Box guide"),
    e!("guide.open.tip", "新規作成・スケッチ・押し出しの短い練習を開く", "Open a short practice: new design, sketch, extrude"),
    e!(
        "guide.rect.body",
        "3D画面で、左ボタンを押したままななめにドラッグして離してください。",
        "In the 3D view, hold the left button, drag diagonally and let go."
    ),
    e!("guide.rect.note", "2回クリックでも描けます。", "Two clicks work too."),
    e!("guide.rect.title", "作業面に四角を描く", "Draw a rectangle on the plane"),
    e!("guide.reset_hints", "隠したヒントを戻す", "Restore hidden hints"),
    e!("guide.sketch.body", "床（XY）の面と四角の道具を自動で選びます。", "The floor (XY) plane and the rectangle tool are chosen for you."),
    e!("guide.sketch.title", "スケッチを始める", "Start the sketch"),
    e!("guide.status.dismiss.tip", "この種類の案内だけ隠す（F1 からいつでも戻せます）", "Hide only this kind of hint (restore it from F1 any time)"),
    e!("guide.status.fix", "直すところ", "To fix"),
    e!("guide.status.fix.where", "（スケッチの輪かくを確認）", "(check the sketch outline)"),
    e!("guide.status.next", "次", "Next"),
    e!("guide.title", "はじめての箱づくり", "Your first box"),
    e!("guide.warn.loft_top", "ロフトの上の輪かくがありません", "The loft has no top outline"),
    e!("guide.warn.loft_top.fix", "上の輪かくを閉じた形で描く", "Draw a closed top outline"),
    e!("guide.warn.no_selection", "操作する形が選ばれていません", "No shape is selected"),
    e!("guide.warn.no_selection.fix", "3D画面か左の一覧で形をクリックしてください", "Click a shape in the 3D view or in the list on the left"),
    e!("guide.warn.nothing_to_cut", "削る元の形がまだありません", "There is nothing to cut yet"),
    e!("guide.warn.nothing_to_cut.fix", "まず「追加」に切り替えて形を作ってください", "Switch to Add first and make a shape"),
    e!("guide.warn.revolve_plane", "回転体はこの面では作れません", "Revolve does not work on this plane"),
    e!("guide.warn.revolve_plane.fix", "描き直して「XZ（正面）」か「YZ（横）」の面で描く", "Redraw on the XZ (Front) or YZ (Side) plane"),
    e!("guide.warn.sweep_path", "スイープの通り道がありません", "The sweep has no path"),
    e!("guide.warn.sweep_path.fix", "「線」で2点以上描いて Enter", "Draw 2 or more points with Line, then Enter"),
    e!(
        "guide.why.closed_outline",
        "ここでは閉じた輪かくが必要です。多角形・四角・円・スプラインを使ってください",
        "A closed outline is needed here. Use Polygon, Rectangle, Circle or Spline"
    ),
    e!(
        "guide.why.loft_top",
        "上側の閉じた輪かくがありません。上の輪かくを描いてください",
        "There is no closed top outline yet. Draw the top outline"
    ),
    e!(
        "guide.why.sweep_path",
        "スイープには2点以上の通り道が必要です。「線」で描いてください",
        "A sweep needs a path of 2 or more points. Draw it with Line"
    ),
    // ----- preferences (the settings branch draws these) -----
    e!("prefs.title", "環境設定", "Preferences", "设置", "환경 설정", "Einstellungen", "Préférences", "Preferencias"),
    e!("prefs.general", "一般", "General", "常规", "일반", "Allgemein", "Général", "General"),
    e!("prefs.appearance", "外観", "Appearance", "外观", "모양", "Darstellung", "Apparence", "Apariencia"),
    e!("prefs.language", "言語", "Language", "语言", "언어", "Sprache", "Langue", "Idioma"),
    e!(
        "prefs.language.hint",
        "画面に表示する言語",
        "Language used by the interface",
        "界面显示语言",
        "화면에 표시할 언어",
        "Sprache der Oberfläche",
        "Langue de l'interface",
        "Idioma de la interfaz"
    ),
    e!("prefs.save", "保存", "Save", "保存", "저장", "Speichern", "Enregistrer", "Guardar"),
    e!("prefs.cancel", "キャンセル", "Cancel", "取消", "취소", "Abbrechen", "Annuler", "Cancelar"),
    e!("prefs.close", "閉じる", "Close", "关闭", "닫기", "Schließen", "Fermer", "Cerrar"),
    e!(
        "prefs.reset",
        "初期設定に戻す",
        "Reset to defaults",
        "恢复默认",
        "기본값으로 되돌리기",
        "Zurücksetzen",
        "Rétablir les valeurs",
        "Restablecer"
    ),
    e!("prefs.restart_hint", "一部の表示は次回起動時に変わります", "Some text changes on the next start"),
    e!("prefs.lang.name.ja", "日本語", "日本語", "日本語", "日本語", "日本語", "日本語", "日本語"),
    e!("prefs.lang.name.en", "English", "English", "English", "English", "English", "English", "English"),
    e!("prefs.lang.name.zh", "简体中文", "简体中文", "简体中文", "简体中文", "简体中文", "简体中文", "简体中文"),
    e!("prefs.lang.name.ko", "한국어", "한국어", "한국어", "한국어", "한국어", "한국어", "한국어"),
    e!("prefs.lang.name.de", "Deutsch", "Deutsch", "Deutsch", "Deutsch", "Deutsch", "Deutsch", "Deutsch"),
    e!("prefs.lang.name.fr", "Français", "Français", "Français", "Français", "Français", "Français", "Français"),
    e!("prefs.lang.name.es", "Español", "Español", "Español", "Español", "Español", "Español", "Español"),
    // ----- materials -----
    e!("material.gradient.title", "グラデーション", "Gradient", "渐变", "그라데이션", "Verlauf", "Dégradé", "Degradado"),
    e!("material.gradient.switch", "グラデーション", "Gradient", "渐变", "그라데이션", "Verlauf", "Dégradé", "Degradado"),
    e!("material.gradient.hint", "2色がなめらかに混ざります", "The two colors blend smoothly"),
    e!("material.gradient.flip", "反転", "Flip", "反转", "반전", "Umkehren", "Inverser", "Invertir"),
    e!("material.gradient.flip.tip", "色の向きを逆にします", "Reverse the color direction"),
    e!("material.gradient.second_color", "2色目", "Second color"),
    e!("material.gradient.follow_shape", "形に合わせる", "Follow shape"),
    e!(
        "material.gradient.follow_shape.tip",
        "オン: 形を回転・拡大しても色は形に付いたまま",
        "On: the color stays on the shape when you rotate or stretch it"
    ),
    // ----- common dialogs and errors -----
    e!(
        "dialog.recovery.title",
        "未保存の作業があります",
        "Unsaved work found",
        "发现未保存的工作",
        "저장하지 않은 작업이 있습니다",
        "Ungespeicherte Arbeit gefunden",
        "Travail non enregistré",
        "Trabajo sin guardar"
    ),
    e!("dialog.recovery.data", "復元データ: ", "Recovery data: "),
    e!("dialog.recovery.note", "復元しても元のファイルは上書きされません。", "Restoring will not overwrite the original file."),
    e!(
        "dialog.recovery.dirty",
        "画面に保存していない作業があります。先に保存してから復元するか、後でを押してください。",
        "There is unsaved work on screen. Save it first, then restore this one (or press Later)."
    ),
    e!(
        "dialog.recovery.restore",
        "未保存のコピーとして復元",
        "Restore as unsaved copy",
        "作为未保存副本恢复",
        "저장하지 않은 사본으로 복원",
        "Als ungespeicherte Kopie wiederherstellen",
        "Restaurer comme copie",
        "Restaurar como copia"
    ),
    e!("dialog.recovery.blocked", "先に画面の作業を保存するか、後でを押してください", "Save the work on screen first, or press Later"),
    e!("dialog.recovery.later", "後で", "Later", "稍后", "나중에", "Später", "Plus tard", "Más tarde"),
    e!(
        "dialog.recovery.delete",
        "復元データを削除",
        "Delete recovery data",
        "删除恢复数据",
        "복원 데이터 삭제",
        "Wiederherstellungsdaten löschen",
        "Supprimer les données",
        "Borrar datos de recuperación"
    ),
    e!("dialog.recovery.restored", "未保存のコピーとして復元しました", "Restored as unsaved copy"),
    e!("dialog.recovery.restored_prefix", "復元しました: ", "Restored: "),
    e!("dialog.recovery.damaged", "復元ファイルが壊れているか、開けません: ", "Recovery file is damaged or cannot be opened: "),
    e!("dialog.recovery.delete_failed", "復元データを削除できませんでした: ", "Could not delete recovery data: "),
    e!(
        "dialog.unsaved.title",
        "保存していない変更があります",
        "You have unsaved changes",
        "有未保存的更改",
        "저장하지 않은 변경이 있습니다",
        "Ungespeicherte Änderungen",
        "Modifications non enregistrées",
        "Cambios sin guardar"
    ),
    e!(
        "dialog.unsaved.body",
        "続ける前に今の作業を保存しますか？",
        "Save current work before continuing?",
        "继续前要保存当前工作吗？",
        "계속하기 전에 저장할까요?",
        "Aktuelle Arbeit vor dem Fortfahren speichern?",
        "Enregistrer le travail avant de continuer ?",
        "¿Guardar el trabajo antes de continuar?"
    ),
    e!(
        "dialog.unsaved.save",
        "保存して続ける",
        "Save and continue",
        "保存并继续",
        "저장하고 계속",
        "Speichern und fortfahren",
        "Enregistrer et continuer",
        "Guardar y continuar"
    ),
    e!(
        "dialog.unsaved.discard",
        "保存せずに続ける",
        "Continue without saving",
        "不保存并继续",
        "저장하지 않고 계속",
        "Ohne Speichern fortfahren",
        "Continuer sans enregistrer",
        "Continuar sin guardar"
    ),
    e!("dialog.unsaved.cancel", "キャンセル", "Cancel", "取消", "취소", "Abbrechen", "Annuler", "Cancelar"),
    e!("error.wait_for_job", "次の操作の前にファイル処理の完了を待ってください", "Wait for the file job to finish before the next action"),
    e!(
        "error.close_file_job",
        "ファイル処理中です。終わるか止めてから閉じてください",
        "A file job is still running. Wait for it or stop it before closing"
    ),
    e!(
        "error.precise_transfer",
        "立体の取り込みが終わるか、取り消してから閉じてください",
        "Wait for the solid import to finish, or cancel it, before closing"
    ),
    e!("error.recovery_cleanup", "復元データを片付けられませんでした: ", "Could not clean up recovery data: "),
    e!(
        "error.active_sketch",
        "シーンを変える前に、実行中のスケッチか立体を終えてください",
        "Finish or cancel the active sketch or solid before changing the scene"
    ),
    e!("error.bool_param", "on は true または false で指定してください", "on must be true or false"),
    e!("error.scene_locked", "入力やドラッグを終えてから画面を切り替えてください", "Finish the input or drag before switching screens"),
    e!("error.rename_saving", "保存が終わるのを待ってから名前を変えてください", "Wait for the save to finish before renaming"),
    // ----- preferences window (settings branch) -----
    e!("settings.open", "設定…", "Settings…", "设置…", "설정…", "Einstellungen…", "Paramètres…", "Ajustes…"),
    e!("settings.title", "設定", "Preferences", "设置", "설정", "Einstellungen", "Préférences", "Preferencias"),
    e!(
        "settings.subtitle",
        "言語・表示・保存の設定",
        "Language, display and saving",
        "语言、显示和保存",
        "언어, 화면 및 저장",
        "Sprache, Anzeige und Speichern",
        "Langue, affichage et enregistrement",
        "Idioma, pantalla y guardado"
    ),
    e!(
        "settings.live_note",
        "変更はすぐに反映されます",
        "Changes apply immediately",
        "更改会立即生效",
        "변경 사항은 즉시 적용됩니다",
        "Änderungen gelten sofort",
        "Les changements s'appliquent immédiatement",
        "Los cambios se aplican al instante"
    ),
    e!("settings.language", "言語", "Language", "语言", "언어", "Sprache", "Langue", "Idioma"),
    e!(
        "settings.language_hint",
        "画面に表示する言語です。選ぶとすぐに切り替わります。",
        "The language used by the app. It changes as soon as you pick one.",
        "选择应用显示的语言，选中后会立即切换。",
        "앱에 표시할 언어를 선택합니다. 선택하면 바로 바뀝니다.",
        "Wähle die Sprache der Anwendung. Sie wird sofort übernommen.",
        "Choisissez la langue de l'application. Elle change dès la sélection.",
        "Elige el idioma de la aplicación. Cambia en cuanto lo seleccionas."
    ),
    e!("settings.units", "単位", "Units", "单位", "단위", "Einheiten", "Unités", "Unidades"),
    e!(
        "settings.units_hint",
        "寸法の表示単位と小数点以下の桁数です。モデルは mm のまま保存されます。",
        "Units and decimal places for dimensions. Models are stored in mm.",
        "尺寸的显示单位和小数位数。模型始终以 mm 保存。",
        "치수의 표시 단위와 소수 자릿수입니다. 모델은 항상 mm로 저장됩니다.",
        "Einheiten und Nachkommastellen für Maße. Modelle werden weiterhin in mm gespeichert.",
        "Unités et décimales des dimensions. Les modèles restent enregistrés en mm.",
        "Unidades y decimales de las dimensiones. Los modelos se guardan en mm."
    ),
    e!("settings.decimals", "小数点以下", "Decimal places", "小数位", "소수 자릿수", "Nachkommastellen", "Décimales", "Decimales"),
    e!("settings.decimals_unit", "桁", "places", "位", "자리", "Stellen", "décimales", "decimales"),
    e!("settings.units_example", "例", "Example", "示例", "예", "Beispiel", "Exemple", "Ejemplo"),
    e!("settings.theme", "テーマ", "Theme", "主题", "테마", "Design", "Thème", "Tema"),
    e!(
        "settings.theme_hint",
        "このアプリはダーク表示のみです。ライトテーマにはまだ対応していません。",
        "This app has a dark theme only; a light theme is not supported yet.",
        "此应用只支持深色主题，暂不支持浅色主题。",
        "이 앱은 어두운 테마만 지원하며 밝은 테마는 아직 지원하지 않습니다.",
        "Diese App hat nur ein dunkles Design; ein helles Design wird noch nicht unterstützt.",
        "Cette application n'a qu'un thème sombre ; le thème clair n'est pas encore pris en charge.",
        "Esta aplicación solo tiene tema oscuro; el tema claro aún no es compatible."
    ),
    e!("settings.theme_dark", "ダーク", "Dark", "深色", "어두운 테마", "Dunkel", "Sombre", "Oscuro"),
    e!("settings.grid", "グリッド", "Grid", "网格", "그리드", "Raster", "Grille", "Rejilla"),
    e!(
        "settings.grid_hint",
        "床のグリッドの表示と間隔です。間隔は mm で指定します。",
        "Show and space the floor grid. Spacing is in mm.",
        "显示地面网格并设置间距。间距以 mm 为单位。",
        "바닥 그리드의 표시와 간격입니다. 간격은 mm 단위입니다.",
        "Bodenraster anzeigen und einstellen. Der Abstand ist in mm.",
        "Afficher et espacer la grille du sol. L'espacement est en mm.",
        "Mostrar y espaciar la rejilla del suelo. El espaciado es en mm."
    ),
    e!("settings.grid_show", "表示", "Show", "显示", "표시", "Anzeigen", "Afficher", "Mostrar"),
    e!("settings.grid_size", "間隔", "Spacing", "间距", "간격", "Abstand", "Espacement", "Espaciado"),
    e!("settings.on", "オン", "On", "开", "켜기", "Ein", "Activé", "Activado"),
    e!("settings.off", "オフ", "Off", "关", "끄기", "Aus", "Désactivé", "Desactivado"),
    e!("settings.snap", "スナップ", "Snap", "吸附", "스냅", "Einrasten", "Accrochage", "Imán"),
    e!(
        "settings.snap_hint",
        "スケッチを描くとき、近くの点やグリッドに吸着します。",
        "When sketching, the cursor sticks to nearby points and the grid.",
        "画草图时，光标会吸附到附近的点和网格。",
        "스케치할 때 커서가 가까운 점과 그리드에 붙습니다.",
        "Beim Skizzieren rastet der Cursor an nahen Punkten und am Raster ein.",
        "Pendant le dessin, le curseur s'accroche aux points proches et à la grille.",
        "Al dibujar, el cursor se pega a los puntos cercanos y a la rejilla."
    ),
    e!("settings.snap_grid", "グリッド", "Grid", "网格", "그리드", "Raster", "Grille", "Rejilla"),
    e!("settings.snap_vertex", "頂点・辺", "Points and edges", "点和边", "점과 모서리", "Punkte und Kanten", "Points et arêtes", "Puntos y aristas"),
    e!("settings.nav", "操作", "Navigation", "导航", "탐색", "Navigation", "Navigation", "Navegación"),
    e!(
        "settings.nav_hint",
        "マウスで視点を動かすときの割り当てと速さです。",
        "Mouse mapping and orbit speed.",
        "鼠标操作和旋转速度。",
        "마우스 조작과 회전 속도입니다.",
        "Mauszuordnung und Drehgeschwindigkeit.",
        "Affectation de la souris et vitesse d'orbite.",
        "Asignación del ratón y velocidad de giro."
    ),
    e!("settings.nav_melt", "Melt", "Melt", "Melt", "Melt", "Melt", "Melt", "Melt"),
    e!("settings.nav_cad", "CAD", "CAD", "CAD", "CAD", "CAD", "CAD", "CAD"),
    e!(
        "settings.nav_melt_help",
        "中央ドラッグで回転、Shift+中央で移動、Ctrl+中央でズーム。右ドラッグでも移動します。",
        "Middle drag orbits, Shift+middle pans, Ctrl+middle zooms. Right drag also pans.",
        "中键拖动旋转，Shift+中键平移，Ctrl+中键缩放。右键拖动也可平移。",
        "가운데 드래그로 회전, Shift+가운데로 이동, Ctrl+가운데로 확대/축소. 오른쪽 드래그로도 이동합니다.",
        "Mittlere Maustaste dreht, Shift+Mitte verschiebt, Ctrl+Mitte zoomt. Rechte Maustaste verschiebt ebenfalls.",
        "Glisser avec le bouton du milieu pour orbiter, Shift+milieu pour déplacer, Ctrl+milieu pour zoomer. Le glisser droit déplace aussi.",
        "Arrastrar con el botón central orbita, Shift+central desplaza, Ctrl+central acerca. Arrastrar con el derecho también desplaza."
    ),
    e!(
        "settings.nav_cad_help",
        "中央ドラッグで移動、Shift+中央で回転、Ctrl+中央でズーム。右ドラッグでも移動します。",
        "Middle drag pans, Shift+middle orbits, Ctrl+middle zooms. Right drag also pans.",
        "中键拖动平移，Shift+中键旋转，Ctrl+中键缩放。右键拖动也可平移。",
        "가운데 드래그로 이동, Shift+가운데로 회전, Ctrl+가운데로 확대/축소. 오른쪽 드래그로도 이동합니다.",
        "Mittlere Maustaste verschiebt, Shift+Mitte dreht, Ctrl+Mitte zoomt. Rechte Maustaste verschiebt ebenfalls.",
        "Glisser avec le bouton du milieu pour déplacer, Shift+milieu pour orbiter, Ctrl+milieu pour zoomer. Le glisser droit déplace aussi.",
        "Arrastrar con el botón central desplaza, Shift+central orbita, Ctrl+central acerca. Arrastrar con el derecho también desplaza."
    ),
    e!("settings.orbit", "回転の速さ", "Orbit speed", "旋转速度", "회전 속도", "Drehgeschwindigkeit", "Vitesse d'orbite", "Velocidad de giro"),
    e!("settings.saving", "保存", "Saving", "保存", "저장", "Speichern", "Enregistrement", "Guardado"),
    e!(
        "settings.saving_hint",
        "作業中の内容を復元用に自動で保存します。",
        "Automatically keeps a recovery copy of your work.",
        "自动保存一份工作副本以便恢复。",
        "작업 내용을 복구용으로 자동 저장합니다.",
        "Hält automatisch eine Wiederherstellungskopie deiner Arbeit.",
        "Garde automatiquement une copie de récupération du travail.",
        "Guarda automáticamente una copia de recuperación del trabajo."
    ),
    e!("settings.autosave", "自動保存", "Autosave", "自动保存", "자동 저장", "Autospeichern", "Enregistrement auto", "Guardado automático"),
    e!("settings.autosave_unit", "分ごと", "minutes", "分钟", "분", "Minuten", "minutes", "minutos"),
    e!(
        "settings.autosave_help",
        "0.5 分 = 30 秒。短いほど安全ですが、書き込みが増えます。",
        "0.5 minutes = 30 seconds. Shorter is safer, but writes more often.",
        "0.5 分钟 = 30 秒。间隔越短越安全，但写入更频繁。",
        "0.5분 = 30초. 짧을수록 안전하지만 더 자주 씁니다.",
        "0,5 Minuten = 30 Sekunden. Kürzer ist sicherer, schreibt aber öfter.",
        "0,5 minute = 30 secondes. Plus court est plus sûr, mais écrit plus souvent.",
        "0,5 minutos = 30 segundos. Más corto es más seguro, pero escribe más a menudo."
    ),
    e!("settings.export", "書き出し", "Export", "导出", "내보내기", "Exportieren", "Exporter", "Exportar"),
    e!(
        "settings.export_hint",
        "新しい書き出しの最初の設定です。書き出し画面でいつでも変えられます。",
        "Starting settings for a new export. You can change them in the export card.",
        "新建导出时的初始设置。可在导出窗口中更改。",
        "새 내보내기의 시작 설정입니다. 내보내기 창에서 언제든 바꿀 수 있습니다.",
        "Startwerte für einen neuen Export. Im Exportfenster jederzeit änderbar.",
        "Réglages de départ d'un nouvel export. Modifiables dans la fenêtre d'export.",
        "Ajustes iniciales de una exportación nueva. Puedes cambiarlos en la ventana de exportación."
    ),
    e!("settings.export_format", "形式", "Format", "格式", "형식", "Format", "Format", "Formato"),
    e!("settings.export_units", "単位", "Units", "单位", "단위", "Einheiten", "Unités", "Unidades"),
    e!("settings.export_up", "上方向", "Up axis", "上方向", "위쪽 축", "Hochachse", "Axe vertical", "Eje vertical"),
    e!(
        "settings.export_up_z",
        "Z（プリンタ向け）",
        "Z (3D printing)",
        "Z（3D 打印）",
        "Z(3D 프린팅)",
        "Z (3D-Druck)",
        "Z (impression 3D)",
        "Z (impresión 3D)"
    ),
    e!(
        "settings.export_up_y",
        "Y（OBJ / GLB 向け）",
        "Y (OBJ / GLB)",
        "Y（OBJ / GLB）",
        "Y(OBJ / GLB)",
        "Y (OBJ / GLB)",
        "Y (OBJ / GLB)",
        "Y (OBJ / GLB)"
    ),
    e!("settings.reset", "初期設定に戻す", "Reset all", "恢复默认设置", "기본 설정으로 되돌리기", "Zurücksetzen", "Réinitialiser", "Restablecer"),
    e!(
        "settings.reset_tip",
        "すべての設定を初期値に戻します",
        "Return every setting to its default",
        "将所有设置恢复为默认值",
        "모든 설정을 기본값으로 되돌립니다",
        "Alle Einstellungen auf die Standardwerte zurücksetzen",
        "Rétablir tous les réglages par défaut",
        "Volver todos los ajustes a sus valores predeterminados"
    ),
    e!("settings.cancel", "キャンセル", "Cancel", "取消", "취소", "Abbrechen", "Annuler", "Cancelar"),
    e!(
        "settings.cancel_tip",
        "開いたときの設定に戻して閉じます",
        "Revert to the settings from when this window opened, then close",
        "恢复到打开此窗口时的设置并关闭",
        "이 창을 열었을 때의 설정으로 되돌리고 닫습니다",
        "Zu den Einstellungen beim Öffnen zurückkehren und schließen",
        "Revenir aux réglages de l'ouverture puis fermer",
        "Volver a los ajustes de la apertura y cerrar"
    ),
    e!("settings.close", "閉じる", "Close", "关闭", "닫기", "Schließen", "Fermer", "Cerrar"),
    e!(
        "settings.close_tip",
        "変更を保ったまま閉じます",
        "Keep the changes and close",
        "保留更改并关闭",
        "변경 사항을 유지하고 닫습니다",
        "Änderungen behalten und schließen",
        "Garder les changements et fermer",
        "Mantener los cambios y cerrar"
    ),
    e!(
        "settings.reset_done",
        "設定を初期値に戻しました",
        "Settings reset to defaults",
        "设置已恢复默认值",
        "설정을 기본값으로 되돌렸습니다",
        "Einstellungen zurückgesetzt",
        "Réglages réinitialisés",
        "Ajustes restablecidos"
    ),
];

fn index() -> &'static HashMap<&'static str, &'static Entry> {
    static INDEX: OnceLock<HashMap<&'static str, &'static Entry>> = OnceLock::new();
    INDEX.get_or_init(|| TABLE.iter().map(|entry| (entry.key, entry)).collect())
}

fn text(entry: &'static Entry, language: Language) -> &'static str {
    let chosen = match language {
        Language::Ja => entry.ja,
        Language::En => entry.en,
        Language::ZhHans => entry.zh,
        Language::Ko => entry.ko,
        Language::De => entry.de,
        Language::Fr => entry.fr,
        Language::Es => entry.es,
    };
    if !chosen.is_empty() {
        chosen
    } else if !entry.en.is_empty() {
        entry.en
    } else if !entry.ja.is_empty() {
        entry.ja
    } else {
        entry.key
    }
}

/// Translate a stable key. A key missing from the table returns itself; a string missing
/// from the chosen language falls back to English, then Japanese. The lifetime follows the
/// key, so a literal key still yields a `'static` translation.
pub fn tr(key: &str) -> &str {
    match index().get(key) {
        Some(entry) => text(entry, CURRENT.with(std::cell::Cell::get)),
        None => key,
    }
}

/// The translated display name of a shape kind. The design data keeps `Kind::label()`.
pub fn kind(kind: crate::model::Kind) -> &'static str {
    use crate::model::Kind;
    tr(match kind {
        Kind::Sphere => "kind.sphere",
        Kind::Box => "kind.box",
        Kind::Cylinder => "kind.cylinder",
        Kind::Torus => "kind.torus",
        Kind::Cone => "kind.cone",
        Kind::Extrude => "kind.extrude",
        Kind::Prism3 => "kind.prism3",
        Kind::PrismN => "kind.prism_n",
        Kind::Octa => "kind.octa",
        Kind::Tetra => "kind.tetra",
        Kind::Pyramid => "kind.pyramid",
        Kind::Capsule => "kind.capsule",
        Kind::Hemi => "kind.hemi",
        Kind::Tube => "kind.tube",
        Kind::Wedge => "kind.wedge",
        Kind::Star => "kind.star",
        Kind::Revolve => "kind.revolve",
        Kind::Loft => "kind.loft",
        Kind::Curve => "kind.curve",
        Kind::Sweep => "kind.sweep",
        Kind::Mesh => "kind.mesh",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Restore Japanese when the test ends, even if it fails.
    struct LangGuard;

    impl Drop for LangGuard {
        fn drop(&mut self) {
            set_language("ja");
        }
    }

    fn lang(code: &str) -> LangGuard {
        set_language(code);
        LangGuard
    }

    #[test]
    fn japanese_is_the_default_language() {
        // A fresh thread has no override; this is what a window starts with.
        let observed = std::thread::spawn(|| (language(), tr("tool.melt"))).join().expect("fresh translation thread");
        assert_eq!(observed, ("ja", "溶かす"));
    }

    #[test]
    fn japanese_keeps_the_existing_text_byte_for_byte() {
        let _ja = lang("ja");
        assert_eq!(tr("inspector.fusion.title"), "Smooth Fusion（なめらか合体）");
        assert_eq!(tr("inspector.fusion.radius"), "接合部の丸み");
        assert_eq!(tr("inspector.fusion.contact.tip"), "0=深く重ねて完全融合、1=離して軽い接触。位置のみ更新、各形の回転とサイズは維持");
        assert_eq!(tr("inspector.fusion.full"), "完全融合");
        assert_eq!(tr("inspector.fusion.light"), "軽い接触");
        assert_eq!(tr("inspector.fusion.hint"), "画面で形を直接ドラッグできます。形を個別に選ぶと位置・回転・大きさ・色を編集できます。");
        assert_eq!(tr("inspector.melt_preset.edit"), "Meltの形を再編集…");
        assert_eq!(tr("inspector.melt_preset.edit_tip"), "本数や垂れ方を変更。手動で動かした液だれは再生成されます");
        assert_eq!(tr("inspector.part.chair"), "Designer Chair の個別パーツ");
        assert_eq!(tr("inspector.part.chair_settings"), "椅子全体の設定");
        assert_eq!(tr("inspector.part.organic_open"), "有機家具の設定を開く");
        assert_eq!(tr("inspector.part.organic_note"), "このパーツは家具プリセットの一部です。設定を変更すると配置を作り直します。");
        assert_eq!(tr("tool.solidcraft.tip"), "SolidCraft: 精密なスケッチと立体を作る。戻ると自動で Melt に取り込み、とけあう形にできます");
        assert_eq!(tr("error.precise_transfer"), "立体の取り込みが終わるか、取り消してから閉じてください");
    }

    #[test]
    fn english_returns_english() {
        let _en = lang("en");
        assert_eq!(tr("tool.melt"), "Melt");
        assert_eq!(tr("inspector.shell"), "Shell");
        assert_eq!(tr("inspector.melt_face"), "Melt one face");
        assert_eq!(tr("inspector.pattern.title"), "Pattern");
        assert_eq!(tr("scene.tab.scene"), "Scene");
        assert_eq!(tr("status.untitled"), "Untitled");
        assert_eq!(tr("prefs.language"), "Language");
        assert_eq!(tr("dialog.unsaved.save"), "Save and continue");
    }

    #[test]
    fn every_language_translates_the_settings_window() {
        // The settings window asks for `settings.*` keys; every one must exist in every catalog.
        let keys: Vec<&str> = TABLE.iter().filter(|entry| entry.key.starts_with("settings.")).map(|entry| entry.key).collect();
        assert_eq!(keys.len(), 50);
        for code in SUPPORTED {
            let _language = lang(code);
            for key in &keys {
                let text = tr(key);
                assert_ne!(text, *key, "{code}: {key} has a translation");
                assert!(!text.trim().is_empty(), "{code}: {key} is not empty");
            }
        }
    }

    #[test]
    fn an_unsupported_language_falls_back_to_japanese() {
        let _xx = lang("tlh-Klingon");
        assert_eq!(language(), "ja");
        assert_eq!(tr("tool.melt"), "溶かす");
        assert_eq!(tr("inspector.shell"), "中抜き");
    }

    #[test]
    fn a_missing_key_falls_back_to_itself_without_panicking() {
        let _en = lang("en");
        assert_eq!(tr("no.such.key"), "no.such.key");
        assert_eq!(tr(""), "");
    }

    #[test]
    fn a_missing_translation_falls_back_to_english_then_japanese() {
        // `inspector.locked` has Japanese and English but no German yet.
        let _de = lang("de");
        assert_eq!(tr("inspector.locked"), "Use the active command card. Finish or cancel to edit other shapes.");
        // `tool.op.add.tip` likewise has no Korean.
        let _ko = lang("ko");
        assert_eq!(tr("tool.op.add.tip"), "Join: the new shape is added to the model (Tab flips Add / Cut)");
    }

    #[test]
    fn every_catalog_has_the_same_keys_and_japanese_and_english_are_complete() {
        let mut seen = std::collections::HashSet::new();
        for entry in TABLE {
            assert!(seen.insert(entry.key), "duplicate translation key: {}", entry.key);
            assert!(!entry.ja.is_empty(), "Japanese is missing for {}", entry.key);
            assert!(!entry.en.is_empty(), "English is missing for {}", entry.key);
            for (name, value) in [("zh-Hans", entry.zh), ("ko", entry.ko), ("de", entry.de), ("fr", entry.fr), ("es", entry.es)] {
                assert!(value.is_empty() || !value.trim().is_empty(), "{name} has blank text for {}", entry.key);
            }
        }
        assert_eq!(SUPPORTED.len(), 7);
    }

    #[test]
    fn no_catalog_string_contains_an_unfilled_format_placeholder() {
        for entry in TABLE {
            for (name, value) in
                [("ja", entry.ja), ("en", entry.en), ("zh-Hans", entry.zh), ("ko", entry.ko), ("de", entry.de), ("fr", entry.fr), ("es", entry.es)]
            {
                assert!(!value.contains('{') && !value.contains('}'), "{name} {} has a brace placeholder: {value}", entry.key);
                assert!(!value.contains('%'), "{name} {} has a percent placeholder: {value}", entry.key);
            }
        }
    }

    #[test]
    fn supported_languages_have_native_names() {
        assert_eq!(language_name("ja"), "日本語");
        assert_eq!(language_name("en"), "English");
        assert_eq!(language_name("zh-Hans"), "简体中文");
        assert_eq!(language_name("ko"), "한국어");
        assert_eq!(language_name("de"), "Deutsch");
        assert_eq!(language_name("fr"), "Français");
        assert_eq!(language_name("es"), "Español");
        assert_eq!(language_name("xx"), "");
    }
}
