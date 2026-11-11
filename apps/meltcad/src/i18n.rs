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
    e!("status.hint.all", "形をドラッグで移動 · 曲線で回転 · 箱で引き伸ばし", "Drag the shape to move · a curve to turn · a box to stretch"),
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
    e!("guide.hint.all", "形をつかんで動かす（矢印・リング・箱でも操作できる）", "Drag the shape to move it (arrows, rings and boxes work too)"),
    e!(
        "guide.hint.all.keys",
        "左ドラッグ: 動かす（Shift: 上下） / 中ドラッグ: 回して見る / ホイール: ズーム",
        "Left drag: move (Shift: up/down) / Middle drag: look around / Wheel: zoom"
    ),
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
    e!(
        "settings.drag_moves",
        "形をドラッグで動かす",
        "Drag a shape to move it",
        "拖动形状来移动",
        "도형을 드래그해 이동",
        "Form durch Ziehen bewegen",
        "Glisser une forme pour la déplacer",
        "Arrastrar una forma para moverla"
    ),
    e!(
        "settings.drag_moves_help",
        "形そのものを左ドラッグすると、選んで床の上をすべらせる（Shift: 上下）。オフにすると、そのドラッグは範囲選択に戻る。",
        "A left drag on a shape selects it and slides it over the floor (Shift: up/down). Off: that drag is a box selection, as before.",
        "在形状上按住左键拖动会选中它并沿地面移动（Shift：上下）。关闭后，该拖动恢复为框选。",
        "도형 위에서 왼쪽 드래그하면 선택하고 바닥 위로 이동합니다(Shift: 위아래). 끄면 그 드래그는 다시 영역 선택이 됩니다.",
        "Linkes Ziehen auf einer Form wählt sie aus und schiebt sie über den Boden (Shift: hoch/runter). Aus: Dieses Ziehen ist wieder eine Rahmenauswahl.",
        "Un glisser gauche sur une forme la sélectionne et la fait glisser sur le sol (Shift : haut/bas). Désactivé : ce glisser redevient une sélection par cadre.",
        "Arrastrar con el botón izquierdo sobre una forma la selecciona y la desliza por el suelo (Shift: arriba/abajo). Desactivado: ese arrastre vuelve a ser una selección por recuadro."
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
    e!("limit.shapes", "形は{0}個まで", "Up to {0} shapes"),
    e!(
        "link.no_add",
        "これはリンクコピーです。形を足すには、元を選ぶかリンクを解除してください",
        "This is a linked copy. Select its source or Unlink it before adding a shape"
    ),
    e!("group.parent_inside", "親グループは、ほかのグループの中に入れられません", "A Parent Group cannot go inside another group"),
    e!("busy.command", "先に、いまの操作を終えるか取り消してください", "Finish or cancel the active command first"),
    e!("group.too_deep", "グループは自分自身の中や、4段より深くには入れられません", "A group cannot go inside itself, or more than 4 levels deep"),
    e!("file.exists", "「{0}」という名前のファイルがすでにあります", "There is already a file named {0}"),
    e!("file.rename_failed", "名前を変えられませんでした: {0}", "Could not rename: {0}"),
    e!("file.busy_save", "ほかのファイル処理が終わってから保存してください", "Wait for the other file job to finish before saving"),
    e!(
        "file.busy_sketch_save",
        "保存の前に、スケッチやソリッドの操作を終えるか取り消してください",
        "Finish or cancel the current sketch or solid operation before saving"
    ),
    e!("file.save_start_failed", "保存を始められませんでした: {0}", "Could not start saving: {0}"),
    e!(
        "file.open_cancelled",
        "待っている間にデザインが変わったため、開くのを取り消しました",
        "The design changed while the open was waiting, so it was cancelled"
    ),
    e!("file.opened", "開きました", "Opened"),
    e!(
        "file.open_not_applied",
        "開いている間にデザインが変わったため、読み込んだファイルは反映していません",
        "The design changed while opening, so the loaded file was not applied"
    ),
    e!("file.saved", "保存しました", "Saved"),
    e!("file.saved_stale", "保存中に編集されました。最新の変更はまだ保存されていません", "Edited while saving: the latest changes are not saved yet"),
    e!("file.unexpected", "ファイル処理が想定外の結果を返しました", "The file job returned an unexpected result"),
    e!("job.stopped", "中止しました", "Stopped"),
    e!("file.open_failed", "開けませんでした: {0}", "Could not open: {0}"),
    e!("file.save_failed", "保存できませんでした: {0}", "Could not save: {0}"),
    e!(
        "file.wait_heavy",
        "ほかの重い処理が終わるのを待ってから開きます（メモリ保護）",
        "Waiting for another heavy job to finish before opening (memory guard)"
    ),
    e!("file.cancel_open", "開くのをやめる", "Cancel open"),
    e!("file.opening", "デザインを開いています…", "Opening the design…"),
    e!("file.saving", "デザインを保存しています…", "Saving the design…"),
    e!("action.stop", "中止", "Stop"),
    e!("file.cannot_stop", "元のファイルを守るため、最後の手順は中止できません", "This last step cannot be stopped, to protect the original file"),
    e!("file.stopping", "中止が終わるのを待っています…", "Waiting for the stop to finish…"),
    e!("menu.new", "新規", "New"),
    e!("menu.open", "開く…  Ctrl+O", "Open…  Ctrl+O"),
    e!("menu.puffy_text", "ぷっくり文字…", "Puffy Text…"),
    e!("menu.puffy_text.tip", "日本語や英語の文字を、ぷっくりした立体にする", "Turn Japanese or English letters into puffy solids"),
    e!("menu.chess", "チェスセット…", "Chess Set…"),
    e!("menu.chess.tip", "3種類のデザインのチェスの駒", "Chess pieces in three designs"),
    e!("menu.save", "保存  Ctrl+S", "Save  Ctrl+S"),
    e!("menu.save_as", "名前を付けて保存…", "Save as…"),
    e!("menu.export_x", "書き出し…", "Export…"),
    e!("menu.about", "Melt 3D について", "About Melt 3D"),
    e!("menu.add_file", "ファイルを追加…", "Add file…"),
    e!(
        "menu.add_file.tip",
        "別の Melt 3D ファイルをここに入れる。いまある形と溶け合う",
        "Put another Melt 3D file into this one. It melts with what is already here"
    ),
    e!("menu.import_svg", "SVGを読み込む…", "Import SVG…"),
    e!("menu.import_mesh", "メッシュを読み込む…", "Import mesh…"),
    e!("menu.export_again", "前回の設定でもう一度書き出す  Ctrl+E", "Export again with last settings  Ctrl+E"),
    e!("menu.export_image", "画像を書き出す (PNG)", "Export image (PNG)"),
    e!("menu.copy", "コピー  Ctrl+C", "Copy  Ctrl+C"),
    e!("menu.cut", "切り取り  Ctrl+X", "Cut  Ctrl+X"),
    e!("menu.paste", "貼り付け  Ctrl+V", "Paste  Ctrl+V"),
    e!("menu.duplicate", "複製  Ctrl+D", "Duplicate  Ctrl+D"),
    e!("menu.array", "並べてコピー…", "Array copy…"),
    e!("menu.clear_pattern", "パターンを消す", "Clear pattern"),
    e!("menu.instance", "リンクコピーを作る", "Make instance"),
    e!("menu.instance.tip", "リンクコピー: 元を編集すると、こちらも変わる", "Linked copy: edits to the original update it too"),
    e!("menu.delete", "削除  Del", "Delete  Del"),
    e!("menu.select_none", "選択を解除  Esc", "Select none  Esc"),
    e!("menu.show_hidden", "隠した形をすべて表示", "Show all hidden"),
    e!("menu.view.front", "前  1", "Front  1"),
    e!("menu.view.right", "右  3", "Right  3"),
    e!("menu.view.top", "上  7", "Top  7"),
    e!("menu.view.back", "後ろ  Ctrl+1", "Back  Ctrl+1"),
    e!("menu.view.left", "左  Ctrl+3", "Left  Ctrl+3"),
    e!("menu.view.bottom", "下  Ctrl+7", "Bottom  Ctrl+7"),
    e!("menu.view.turn_left", "左へ回す  Alt+←", "Turn left  Alt+←"),
    e!("menu.view.turn_right", "右へ回す  Alt+→", "Turn right  Alt+→"),
    e!("menu.view.persp", "遠近あり  5", "Perspective view  5"),
    e!("menu.view.ortho", "遠近なし（平行）  5", "Flat (ortho) view  5"),
    e!("menu.view.frame_all", "全体を表示  Home", "Frame all  Home"),
    e!("menu.view.frame_sel", "選択を表示  Shift+F", "Frame selection  Shift+F"),
    e!("top.undo.tip", "元に戻す (Ctrl+Z)", "Undo (Ctrl+Z)"),
    e!("top.redo.tip", "やり直す (Ctrl+Shift+Z)", "Redo (Ctrl+Shift+Z)"),
    e!("export.menu.tip", "STL・3MF・OBJ・PLY・GLB。細かさ・単位・色", "STL, 3MF, OBJ, PLY, GLB. Detail, units, color"),
    e!("export.again", "もう一度書き出す", "Export again"),
    e!("export.image", "画像 (PNG)", "Image (PNG)"),
    e!("export.image.tip", "画面に見えているとおりに保存", "Save as seen on screen"),
    e!("export.print_check", "プリント前チェック", "Print check"),
    e!("export.print_check.tip", "大きさ・体積・穴", "Size, volume, holes"),
    e!("device.mouse", "マウス", "Mouse"),
    e!("device.mouse.tip", "ホイールでズーム、中ドラッグで回転、右ドラッグで移動", "Wheel zooms, middle-drag turns, right-drag slides"),
    e!("device.trackpad", "トラックパッド", "Trackpad"),
    e!("device.trackpad.tip", "2本指で回転（Shift: 移動）、ピンチでズーム", "Two fingers turn (Shift: slide), pinch zooms"),
    e!("top.rename.tip", "クリックでデザインの名前を変える", "Click to rename this design"),
    e!("link.copy", "リンクコピー", "Linked copy"),
    e!("pattern.remove", "パターンを消す", "Remove pattern"),
    e!("keys.ctrl_drag", "Ctrl + ドラッグ", "Ctrl + drag"),
    e!("keys.steps", "1 mm / 15° 刻み", "1 mm / 15° steps"),
    e!("keys.views.k", "1 3 7（Ctrl: 反対側）", "1 3 7 (Ctrl: opposite)"),
    e!("outline.ball", "ボール {0}   半径 {1}", "Ball {0}   r {1}"),
    e!("size.radius", "半径", "Radius"),
    e!("size.width", "幅", "Width"),
    e!("size.depth", "奥行き", "Depth"),
    e!("size.height", "高さ", "Height"),
    e!("size.major_radius", "輪の半径", "Major radius"),
    e!("size.minor_radius", "太さの半径", "Minor radius"),
    e!("size.outer_radius", "外側の半径", "Outer radius"),
    e!("size.base_radius", "底の半径", "Base radius"),
    e!("size.length", "長さ", "Length"),
    e!("size.wall", "壁の厚み", "Wall"),
    e!("size.sides", "辺の数", "Sides"),
    e!("size.points", "とがりの数", "Points"),
    e!("size.inner_ratio", "内側の比率", "Inner ratio"),
    e!("size.angle", "角度", "Angle"),
    e!("keys.title", "操作一覧", "Controls"),
    e!("keys.middle_drag", "中ドラッグ", "Middle drag"),
    e!("keys.orbit", "回して見る", "Orbit"),
    e!("keys.right_drag", "右ドラッグ", "Right drag"),
    e!("keys.pan", "視点を移動", "Pan"),
    e!("keys.wheel", "ホイール", "Wheel"),
    e!("keys.zoom", "カーソルに向かってズーム", "Zoom to cursor"),
    e!("keys.left_drag_empty", "左ドラッグ（何もない所）", "Left drag (empty)"),
    e!("keys.box_select", "範囲選択", "Box select"),
    e!("keys.left_drag_shape", "左ドラッグ（形の上）", "Left drag (on a shape)"),
    e!("keys.move_floor", "床の上を動かす  Shift: 上下", "Move on the floor  Shift: up/down"),
    e!("keys.alt_drag", "Alt + 左ドラッグ", "Alt + left drag"),
    e!("keys.click", "クリック", "Click"),
    e!("keys.select", "選択  Ctrl: 追加/解除  Shift: 追加/解除", "Select  Ctrl: add/remove  Shift: add/remove"),
    e!("keys.double_click", "ダブルクリック", "Double click"),
    e!("keys.focus", "形に寄る", "Focus on shape"),
    e!("keys.drawing", "描くとき: 左クリック → 右クリック", "Drawing: left click → right click"),
    e!("keys.drawing.v", "点を置く → 確定（描く道具すべて）", "Place point → finish (all draw tools)"),
    e!("keys.find", "形・ライブラリ・コマンドを探す", "Find shapes, library, commands"),
    e!("keys.frame", "選択を表示 / 全体を表示", "Frame selection / Frame all"),
    e!("keys.views", "前 / 右 / 上", "Front / Right / Top"),
    e!("keys.persp", "遠近あり ⇄ なし", "Perspective ⇄ Ortho"),
    e!("keys.add_sub", "足す ⇄ 引く", "Add ⇄ Subtract"),
    e!("keys.arrows", "矢印キー", "Arrow keys"),
    e!("keys.nudge", "1 mm 動かす（Shift: 10 mm）", "Move 1 mm (Shift: 10 mm)"),
    e!("keys.clipboard", "コピー / 切り取り / 貼り付け", "Copy / Cut / Paste"),
    e!("keys.dup_del", "複製 / 削除", "Duplicate / Delete"),
    e!("keys.group", "選んだ形をグループにする", "Group the selected shapes"),
    e!("keys.undo", "元に戻す / やり直す", "Undo / Redo"),
    e!("image.saved", "画像を保存しました", "Image saved"),
    e!("image.save_failed", "画像を保存できませんでした: {0}", "Could not save image: {0}"),
    e!("picture.busy", "画像を作っている途中です。終わるまで待ってください", "A picture is already being drawn; wait for it to finish"),
    e!("saved.path", "{0} を保存しました", "Saved {0}"),
    e!("picture.failed", "画像を保存できませんでした: {0}", "Picture not saved: {0}"),
    e!("recovery.save_failed", "自動バックアップを保存できませんでした: {0}", "Could not save auto-recovery data: {0}"),
    e!("melt.preset.firm", "カチッと", "Firm"),
    e!("melt.preset.soft", "やわらか", "Soft"),
    e!("melt.preset.bouncy", "ぷるぷる", "Bouncy"),
    e!("melt.preset.cheese", "チーズ", "Cheese"),
    e!("melt.preset.honey", "はちみつ", "Honey"),
    e!("melt.preset.runny", "とろとろ", "Runny"),
    e!("group.put_inside", "中に入れる", "Put inside"),
    e!(
        "fusion.changed",
        "シーンの溶かし設定が変わったため、なめらか融合の見た目が変わっています",
        "The scene's Melt settings changed, so Smooth Fusion looks different"
    ),
    e!("fusion.restore", "なめらか融合の溶かし方に戻す", "Back to the Smooth Fusion melt"),
    e!("group.edit_parts", "パーツを編集", "Edit the parts"),
    e!("group.edit_parts.tip", "最初の形を選びます。ほかの形は左のリストで選べます", "Selects the first shape; pick the others in the scene list"),
    e!(
        "solid.edit.tip",
        "このソリッドを SolidCraft で開く（リストでダブルクリックでも可）。直して戻ると、その場で置き換わる",
        "Open this solid in SolidCraft (or double-click it in the list). Change it, go back, and it is replaced where it stands"
    ),
    e!("ripple.title", "波もよう", "Ripple"),
    e!("ripple.use", "波もようを使う", "Use ripple"),
    e!("ripple.rings", "輪", "Rings"),
    e!("ripple.lines", "線", "Lines"),
    e!("ripple.random", "ランダム", "Random"),
    e!("ripple.height", "波の高さ", "Wave height"),
    e!("ripple.height.tip", "波の高さ (mm)", "Amplitude (mm)"),
    e!("ripple.spacing", "波の間隔", "Wave spacing"),
    e!("ripple.spacing.tip", "波から波までの長さ (mm)", "Wavelength (mm)"),
    e!("ripple.waves", "波の数", "Waves"),
    e!("ripple.direction.tip", "線・ランダムの波の向き（度）", "Direction of line and random waves (degrees)"),
    e!("ripple.fade", "端のぼかし", "Edge fade"),
    e!("ripple.fade.tip", "波が届く範囲の端で、どれだけなめらかに消えるか", "How smoothly the waves fade out at the edge of their range"),
    e!("ripple.keep_outline", "輪郭を保つ（表面を内側に彫る）", "Keep the outline (carve the surface inward)"),
    e!("ripple.detail", "表面の細かさ", "Surface detail"),
    e!(
        "ripple.detail.tip",
        "作業中は軽いプレビュー。書き出しは選んだ精度で行う",
        "A light preview while you work; the export uses the chosen precision"
    ),
    e!("preview.confirm", "プレビューを調整して、右クリックで確定", "Adjust the preview, then right-click to confirm"),
    e!("material.select_first", "形を選ぶと、ここで素材を選べます", "Select a shape to choose its material here"),
    e!("material.custom", "カスタム", "Custom"),
    e!("material.weight", "重さ: {0}", "Weight: {0}"),
    e!(
        "material.weight.tip",
        "この素材がグラフィックにかける負荷。ガラス・光のしみこみ・コート・つやは重い",
        "How hard this material is for the graphics card. Glass, soaking light, coat and sheen are heavy"
    ),
    e!("material.already_saved", "★ マイ素材に保存済みです", "Already saved under ★ My"),
    e!(
        "material.save_limit",
        "保存できる数の上限です。保存した素材を右クリックで消せます",
        "Save limit reached. Right-click a saved material to remove it"
    ),
    e!("material.apply_group", "グループ全体に使う", "Apply to whole group"),
    e!("material.lighter", "軽くする", "Lighter"),
    e!(
        "material.lighter.tip",
        "選んだ形の透明・しみこみ・コート・つやを切る。ずっと速くなる",
        "Switch off see-through, soaking, coat and sheen on the selected shapes: much faster"
    ),
    e!("material.all_lighter", "全部軽くする", "All lighter"),
    e!("material.all_lighter.tip", "シーンの素材をすべて軽くする", "Make every material in the scene light"),
    e!("material.random", "ランダムな色", "Random color"),
    e!("material.random.tip", "選んだ形それぞれに新しいランダムな色", "A new random color for each selected shape"),
    e!("material.random_all", "全部ランダム", "Random all"),
    e!("material.random_all.tip", "すべての形にランダムな色", "A random color for every shape"),
    e!("material.light_first", "軽い順", "Light first"),
    e!("material.light_first.tip", "素材を軽い順に並べる", "Order the materials from lightest to heaviest"),
    e!("material.save_hint", "★ 保存で、素材をここに残せます", "Use ★ Save to keep materials here"),
    e!("material.saved.tip", "クリックで使う / 右クリックで消す", "Click to use / right-click to remove"),
    e!("material.details", "くわしく", "Details"),
    e!("material.color", "色", "Color"),
    e!("material.gloss", "つや", "Gloss"),
    e!("material.gloss.tip", "つるつる ⇄ ざらざら", "Smooth ⇄ Rough"),
    e!("material.metallic", "金属感", "Metallic"),
    e!("material.metallic.tip", "プラスチック ⇄ 金属", "Plastic ⇄ Metal"),
    e!("material.transparent", "透明", "Transparent"),
    e!("material.transparent.tip", "ガラスのように光を曲げて、向こうが透ける", "Bends light like glass and lets you see through"),
    e!("material.glow", "発光", "Glow"),
    e!("material.glow.tip", "自分で光る（ランプ、ネオン）", "Emits its own light (lamp, neon)"),
    e!("material.iridescent", "玉虫色", "Iridescent"),
    e!(
        "material.iridescent.tip",
        "見る角度で虹色に変わる（シャボン玉、コガネムシ）",
        "Shifts rainbow colors with the viewing angle (soap bubble, beetle)"
    ),
    e!("material.coat", "コート", "Coat"),
    e!("material.coat.tip", "つやのある透明な膜（車の塗装、あめ）", "Glossy clear layer (car paint, candy)"),
    e!("material.translucency", "光のしみこみ", "Translucency"),
    e!("material.translucency.tip", "光が中にしみこむ（ろう、ゼリー、氷）", "Light soaks in (wax, jelly, ice)"),
    e!("room.look", "見た目", "Look"),
    e!("room.backdrop", "背景", "Backdrop"),
    e!("room.floor_shadow", "床の影", "Floor shadow"),
    e!("room.exposure", "明るさ", "Exposure"),
    e!("room.exposure.tip", "カメラの露出のような明るさ", "Brightness, like camera exposure"),
    e!("room.shadow", "影の濃さ", "Shadow strength"),
    e!("room.ambient", "まわりの光", "Ambient"),
    e!("room.ambient.tip", "影と空の映りこみの明るさ", "Brightness of shadows and sky reflections"),
    e!("room.sky", "空（金属やガラスに映る）", "Sky (reflected in metal and glass)"),
    e!("outline.spinning_top", "こま", "Top"),
    e!("room.sky.top", "上", "Top"),
    e!("room.sky.horizon", "地平線", "Horizon"),
    e!("room.sky.bottom", "下", "Bottom"),
    e!("room.light_n", "ライト {0}", "Light {0}"),
    e!("action.remove", "消す", "Remove"),
    e!("room.light.remove", "このライトを消す", "Remove this light"),
    e!("room.light.turn", "まわりこみ（度）", "Turn around (degrees)"),
    e!("room.light.elevation", "高さの角度", "Elevation"),
    e!("room.light.elevation.tip", "低い ⇄ 真上（度）", "Low ⇄ Overhead (degrees)"),
    e!("room.light.power", "強さ", "Power"),
    e!("room.light.shadow_note", "影を落とすのはライト 1 だけです", "Only Light 1 casts shadows"),
    e!("outline.profile", "断面（半分）", "Profile (half)"),
    e!("outline.bottom", "下の輪郭", "Bottom outline"),
    e!("outline.top", "上の輪郭", "Top outline"),
    e!("outline.sketch", "スケッチ（輪郭）", "Sketch (outline)"),
    e!(
        "outline.revolve.tip",
        "X = 中心軸からの距離、Y = 高さ。軸のまわりに回転させる",
        "X = distance from the center axis, Y = height. Revolves around the axis"
    ),
    e!("outline.point.tip", "ドラッグで移動 / ダブルクリックで削除", "Drag to move / double-click to remove"),
    e!("outline.add_corner", "辺をクリックで角を追加  {0}/{1}", "Click an edge to add a corner  {0}/{1}"),
    e!("outline.square", "四角", "Square"),
    e!("outline.triangle", "三角", "Triangle"),
    e!("outline.hexagon", "六角", "Hexagon"),
    e!("outline.star", "星", "Star"),
    e!("outline.cross", "十字", "Cross"),
    e!("outline.vase", "花びん", "Vase"),
    e!("outline.cup", "カップ", "Cup"),
    e!("outline.square_cyl", "角柱", "Square cylinder"),
    e!("melt.amount", "溶け具合", "Amount"),
    e!("melt.amount.tip", "すべての形の溶け具合をまとめて変える", "Melt strength for all shapes at once"),
    e!("melt.drip", "たれ", "Drip"),
    e!("melt.drip.tip", "下に向かって、たれるように溶ける", "Melts downward, like dripping"),
    e!("melt.drip_spacing", "たれの間隔", "Drip spacing"),
    e!("melt.drip_spacing.tip", "たれとたれの間隔", "Spacing of the drips"),
    e!("melt.per_shape_note", "形ごとの溶け具合は「形」タブで決めます", "Melt per shape is set in the Shape tab"),
    e!("preview.scene", "プレビュー: シーン全体", "Preview: whole scene"),
    e!("preview.selected", "プレビュー: 選択中", "Preview: selected"),
    e!("preview.title", "プレビュー", "Preview"),
    e!("gradient.lr", "左右", "Left-Right"),
    e!("gradient.lr.tip", "X: 左 → 右", "X: left → right"),
    e!("gradient.fb", "前後", "Front-Back"),
    e!("gradient.fb.tip", "Y: 前 → 後ろ", "Y: front → back"),
    e!("gradient.ud", "上下", "Up-Down"),
    e!("gradient.ud.tip", "Z: 下 → 上", "Z: bottom → top"),
    e!("gradient.softness", "やわらかさ", "Softness"),
    e!("gradient.strength", "強さ", "Strength"),
    e!("gradient.strength.tip", "終わりの色がどれだけ出るか", "How much of the end color shows"),
    e!("gradient.from", "始まり", "From"),
    e!("gradient.from.tip", "色が変わり始める位置（0 = 端）", "Where the color starts to change (0 = edge)"),
    e!("gradient.to", "終わり", "To"),
    e!("gradient.to.tip", "色が変わり終わる位置（1 = 反対の端）", "Where the color finishes changing (1 = opposite edge)"),
    e!("pipe.hint", "1回なぞってパイプを描く", "Drag once to draw a pipe"),
    e!("pipe.stop.tip", "パイプを描くのをやめる（Esc または右クリック）", "Stop drawing pipes (Esc or right click)"),
    e!("pipe.fix", "選んだパイプを直す", "Fix the selected pipe"),
    e!("action.done", "完了", "Done"),
    e!("pipe.done.tip", "このカードをしまう (Esc)", "Put this card away (Esc)"),
    e!("pipe.radius", "外側の半径", "Outer radius"),
    e!("pipe.radius.tip", "パイプの外側の半径 (mm)", "Outer radius of the pipe (mm)"),
    e!("action.less", "少なく", "Less"),
    e!("action.more", "もっと…", "More…"),
    e!("pipe.more.tip", "厚み・端・輪にする", "Wall, ends, closed loop"),
    e!("pipe.wall", "壁の\n厚み", "Wall\nthickness"),
    e!("pipe.loop", "輪にする", "Closed loop"),
    e!("pipe.loop.tip", "最後の点と最初の点をつなぐ", "Join the last and first node"),
    e!("pipe.ends", "端", "Ends"),
    e!("pipe.ends.round.tip", "端を丸く閉じる", "Close the ends round"),
    e!("pipe.ends.flat", "平ら", "Flat"),
    e!("pipe.ends.flat.tip", "端をまっすぐ切る（穴が見える）", "Cut the ends straight (shows the hole)"),
    e!("pipe.plane", "なぞる面", "Trace plane"),
    e!("pipe.plane.tip", "形がない所では、この面の上をなぞる", "Where there is no shape, trace on this plane"),
    e!("pipe.note", "形の上では表面に沿う · 中ドラッグで視点を回す", "Over a shape it follows the surface · middle drag turns the view"),
    e!("pattern.select_first", "先にグループか形を選んでください", "Select a group or shape first"),
    e!(
        "pattern.no_path",
        "沿わせる線がありません（先に線・スイープ・押し出しを作ってください）",
        "No path to follow (make a line, sweep or extrusion first)"
    ),
    e!("pattern.no_source", "点を借りられる形がほかにありません", "No other shape to borrow points from"),
    e!("pattern.already", "別のパターンがすでにオンです。先にオフにしてください", "Another pattern is already on. Turn it off first"),
    e!("limit.groups", "グループは{0}個まで", "Up to {0} groups"),
    e!("link.made", "リンクコピーを作りました。元を編集すると、こちらも変わります", "Made a linked copy. Edits to the original also change this one"),
    e!("pattern.mirror.x", "Xで鏡写し（左右）", "Mirror across X (left/right)"),
    e!("pattern.mirror.y", "Yで鏡写し（前後）", "Mirror across Y (front/back)"),
    e!("pattern.mirror.z", "Zで鏡写し（上下）", "Mirror across Z (up/down)"),
    e!("pattern.intro", "この{0}をたくさん並べたり、左右に鏡写しにしたりする", "Repeat this {0} many times, or mirror it left and right"),
    e!("pattern.origin.tip", "原点を通る面", "The plane through the world origin"),
    e!("pattern.own_center", "自分の中心", "Own center"),
    e!("pattern.own_center.tip", "このグループや形の中心を通る面", "The plane through this group's or shape's own center"),
    e!("pattern.count_axis", "数 {0}", "Count {0}"),
    e!("pattern.count_axis.tip", "この軸に並べる数（1 = これだけ）", "Copies along this axis (1 = just this one)"),
    e!("pattern.spacing_axis", "間隔 {0}", "Spacing {0}"),
    e!(
        "pattern.spacing_axis.tip",
        "この軸でのコピーの間隔（mm。マイナスで逆向き）",
        "Distance between copies along this axis (mm; a minus goes the other way)"
    ),
    e!("pattern.count_circle.tip", "円に並べる数", "Copies around the circle"),
    e!("pattern.angle.tip", "コピーが回る範囲（度）", "How much of a turn the copies cover (degrees)"),
    e!("pattern.radius", "半径", "Radius"),
    e!("pattern.radius.tip", "真ん中からコピーまでの距離 (mm)", "How far from the middle the copies go (mm)"),
    e!("pattern.path_label", "線", "Path"),
    e!("pattern.layout", "並べ方", "Layout"),
    e!("pattern.even", "均等", "Even"),
    e!("pattern.even.tip", "線の端から端まで均等に", "Evenly from one end of the line to the other"),
    e!("pattern.distance", "距離", "Distance"),
    e!("pattern.distance.tip", "決めた長さごとに", "Every set length"),
    e!("pattern.at_most", "最大", "At most"),
    e!("pattern.at_most.tip", "線に沿って置く数の上限", "Most copies to put along the path"),
    e!("pattern.gap", "すき間", "Gap"),
    e!("pattern.gap.tip", "線に沿ったコピーどうしの距離 (mm)", "Distance from one copy to the next along the path (mm)"),
    e!("pattern.align_line", "線の向きに合わせる", "Align to the line's direction"),
    e!("pattern.borrow", "点を借りる", "Borrow points"),
    e!("pattern.group_name", "グループ: {0}", "Group: {0}"),
    e!("pattern.align_point", "点の向きに合わせる", "Align to the point's direction"),
    e!(
        "pattern.borrow.note",
        "線・スイープ・押し出しは点と角、箱は8つの角、ほかは中心を使います",
        "Lines, sweeps and extrusions use nodes and corners, boxes use 8 corners, others use the center"
    ),
    e!("pattern.current_count", "いまの数", "Current count"),
    e!("pattern.path_missing", "線が見つかりません（線か形）", "No path found (line or shape)"),
    e!(
        "pattern.frame_note",
        "形の枠は使いません。コピーを選ぶと、パターンの元の形やグループが選ばれます",
        "The shape frame is not used. Choosing a copy selects the shape or group the pattern was made from"
    ),
    e!("link.source", "元", "Source"),
    e!("link.no_source", "元のグループがありません", "No source group"),
    e!("link.pick_source", "元を選ぶ", "Pick source"),
    e!("link.unlink", "リンクを解除", "Unlink"),
    e!("link.unlink.tip", "ふつうの形としてコピーする（元とのリンクはなくなる）", "Copies it as a plain shape (no longer linked to the source)"),
    e!("link.note", "元を編集すると、リンクコピーも変わります", "Edits to the source also change the linked copy"),
    e!("curve.thickness", "太さ", "Thickness"),
    e!("curve.thickness.tip", "新しい点の半径 (mm)", "Radius of new nodes (mm)"),
    e!("curve.done.tip", "この曲線を終えて、道具をしまう (Esc)", "Finish this curve and leave the tool (Esc)"),
    e!(
        "curve.hint",
        "左クリック: 点を追加   右クリック / Enter: 完了   Backspace: 点を1つ戻す",
        "Left-click: add node   Right-click / Enter: done   Backspace: undo one node"
    ),
    e!("curve.nodes", "曲線の点", "Curve nodes"),
    e!("curve.path_nodes", "線の点", "Path nodes"),
    e!("curve.edit_nodes", "点を編集", "Edit nodes"),
    e!("curve.edit_nodes.tip", "画面の点をドラッグして動かす", "Drag nodes on screen to move them"),
    e!("curve.extend.tip", "端の + をドラッグで伸ばす、点の間の + で追加", "Drag the + at an end to extend, the + between nodes to add"),
    e!("curve.tangent", "つながり方", "Tangent"),
    e!("curve.auto", "自動", "Auto"),
    e!("curve.smooth", "なめらか", "Smooth"),
    e!("curve.corner", "角", "Corner"),
    e!("curve.smoothness", "なめらかさ", "Smoothness"),
    e!("curve.end_round", "端の丸み", "End roundness"),
    e!("curve.section", "断面", "Section"),
    e!("curve.soft_square", "丸い四角", "Soft square"),
    e!("curve.overall", "全体の太さ", "Overall thickness"),
    e!("curve.same", "太さをそろえる", "Same thickness"),
    e!("curve.same.tip", "選んだ点（なければ最初の点）の太さにそろえる", "Match the selected node's thickness (or the first)"),
    e!("curve.node_n", "点 {0}", "Node {0}"),
    e!("curve.node.delete", "この点を消す", "Delete this node"),
    e!("curve.node", "点", "Node"),
    e!("curve.add_end", "端に追加", "Add at the end"),
    e!("gif.no_result", "GIFの処理が結果なしで終わりました", "GIF worker ended without a result"),
    e!("gif.failed", "GIFを保存できませんでした: {0}", "GIF not saved: {0}"),
    e!("gif.making", "GIFを作成中 {0}/{1}", "Making GIF {0}/{1}"),
    e!("gif.cancel", "GIFをやめる", "Cancel GIF"),
    e!("anim.show_timeline", "タイムラインを表示", "Show timeline"),
    e!("anim.play", "再生", "Play"),
    e!("anim.play.tip", "アニメーションを再生 (Space)", "Play the animation (Space)"),
    e!("anim.busy_play", "再生の前に、スケッチやドラッグを終えるか取り消してください", "Finish or cancel the sketch or drag before playing"),
    e!("anim.first", "最初のコマに戻る", "Back to the first frame"),
    e!("anim.set_pose", "ポーズを記録", "Set pose"),
    e!("anim.set_pose.tip", "いまのシーンを、このコマのポーズとして残す", "Keep the scene as it is now at this frame"),
    e!(
        "anim.busy_pose",
        "ポーズを記録する前に、スケッチやドラッグを終えるか取り消してください",
        "Finish or cancel the sketch or drag before recording a pose"
    ),
    e!("anim.delete_pose", "ポーズを消す", "Delete pose"),
    e!("anim.delete_pose.tip", "このコマのポーズを消す", "Forget the pose at this frame"),
    e!("anim.frame", "コマ {0} / {1}", "Frame {0} / {1}"),
    e!("anim.speed", "速さ", "Speed"),
    e!("anim.length", "長さ", "Length"),
    e!("anim.clay", "クレイ", "Clay"),
    e!("anim.clay.tip", "コマ撮りのように、各ポーズを数コマ止める", "Hold each pose for a few frames, like stop motion"),
    e!("gif.making2", "GIFを作成中 {0}/{1}", "Making GIF {0}/{1}"),
    e!("gif.export", "GIFを書き出す", "Export GIF"),
    e!("gif.export.tip", "アニメーションを動く画像として保存", "Save the animation as a moving picture"),
    e!("library.model_failed", "このモデルを読み込めませんでした。", "This model could not be loaded."),
    e!("library.thumb_failed", "画像を作れませんでした。", "The picture could not be made."),
    e!("library.add", "{0} をシーンに追加", "Add {0} to the scene"),
    e!(
        "library.added_smooth",
        "{0} を追加しました。シーン全体の溶かし方が「なめらか」になりました",
        "Added {0}; the whole scene's Melt is now Smooth"
    ),
    e!("library.added", "{0} を追加しました", "Added {0}"),
    e!("library.search", "名前で探す", "Search by name"),
    e!("library.search_partial", "{0}個のカテゴリの検索データを読めませんでした", "Could not read search data for {0} categories"),
    e!("library.search_preparing", "検索データを準備しています…", "Preparing search data…"),
    e!("library.search_failed", "検索データを読み込めませんでした", "Could not load search data"),
    e!("library.category_failed", "このカテゴリを開けませんでした: {0}", "Could not open this category: {0}"),
    e!("library.loading", "モデルを読み込んでいます…", "Loading models…"),
    e!("library.load_failed", "モデルを読み込めませんでした", "Could not load models"),
    e!("library.none", "見つかりません", "No results"),
    e!("library.model_error", "このモデルを読み込めませんでした: {0}", "Could not load this model: {0}"),
    e!(
        "library.still_loading",
        "モデルを読み込み中です。読み込みが終わってから追加してください",
        "The model is still loading; add it once it has loaded"
    ),
    e!(
        "sketch.finish_first",
        "ソリッドの操作を選ぶ前に、先にスケッチを終えてガイドを保存してください",
        "Finish Sketch first to save the guide before choosing a solid operation"
    ),
    e!("solid.open", "このソリッドを SolidCraft で開く", "Open this solid in SolidCraft"),
    e!("clip.newer", "このコピーは新しいバージョンのものです", "This copy is from a newer version"),
    e!("clip.empty", "中身が空でした", "It was empty"),
    e!("clip.pasted", "貼り付けました", "Pasted"),
    e!("clip.pasted_groups", "{0}個のグループを貼り付けました", "Pasted {0} groups"),
    e!("clip.nothing", "貼り付けるものがありません", "Nothing to paste"),
    e!(
        "link.no_paste",
        "これはリンクコピーです。形を貼り付けるには、元を選ぶかリンクを解除してください",
        "This is a linked copy. Select its source or Unlink it before pasting shapes"
    ),
    e!("clip.too_many", "IDが多すぎて、安全に貼り付けられません", "Too many identifiers to paste safely"),
    e!(
        "clip.copied_first",
        "選んだ{0}個のうち最初の1つをコピーしました（コピーできるのは1つです）",
        "Copied the first of {0} selected (a copy holds one thing)"
    ),
    e!("clip.copied", "コピーしました", "Copied"),
    e!("clip.select_first", "コピーするものを選んでください", "Select something to copy"),
    e!(
        "clip.cut_one",
        "切り取りは1つずつです。1つ選ぶか、まとめて消すなら Delete を押してください",
        "Cut takes one thing at a time. Select one, or press Delete to remove several"
    ),
    e!("clip.none_yet", "まだ何もコピーしていません", "Nothing copied yet"),
    e!("array.min", "2以上にしてください", "Please use 2 or more"),
    e!("array.no_shape", "形がありません", "No shape"),
    e!("array.no_group", "グループがありません", "No group"),
    e!("array.over_limit", "上限を超えます", "That goes over the limit"),
    e!("array.select_first", "並べるものを選んでください", "Select something to arrange"),
    e!("array.title", "並べてコピー", "Array copy"),
    e!("array.tip", "選んだ形（グループ）をたくさん並べる", "Repeats the selected shape (group) many times"),
    e!("array.straight", "まっすぐ", "Straight"),
    e!("array.circle", "円（Z軸まわり）", "Circle (around Z)"),
    e!("array.total", "合計", "Total"),
    e!("array.width", "幅 X", "Width X"),
    e!("array.depth", "奥行き Y", "Depth Y"),
    e!("array.height", "高さ Z", "Height Z"),
    e!("array.button", "並べる", "Array"),
    e!("array.added", "{0}個のコピーを追加しました", "Added {0} copies"),
    e!("quality.light", "軽い", "Light"),
    e!("quality.normal", "ふつう", "Normal"),
    e!("quality.sharp", "くっきり", "Sharp"),
    e!("quality.auto", "自動", "Auto"),
    e!("quality.light.tip", "いちばん軽い（画面の細かさを半分に）", "Lightest (half the screen detail)"),
    e!("quality.normal.tip", "中くらい（細かさ4分の3）", "Medium (three-quarters detail)"),
    e!("quality.sharp.tip", "いつも最高の細かさ", "Always full detail"),
    e!(
        "quality.auto.tip",
        "止まっているときはくっきり、動かしている間は軽くして速さを保つ",
        "Sharp when still; lighter while moving to keep up the speed"
    ),
    e!("quality.menu", "画質: {0}", "Quality: {0}"),
    e!("quality.hud", "描画時間を表示 (F3)", "Show frame time (F3)"),
    e!("quality.speed_test", "速度テスト…", "Speed test…"),
    e!("viewmode.zebra_h", "しま模様（横）", "Zebra (horizontal stripes)"),
    e!("viewmode.zebra_v", "しま模様（縦）", "Zebra (vertical stripes)"),
    e!("viewmode.rings", "等高線の輪", "Rings (contour bands)"),
    e!("viewmode.updown", "上向き / 下向き（緑が上、赤が下、青 = 垂直の壁）", "Up / Down (green up, red down, blue = vertical wall)"),
    e!("sketch_ui.plane_floor_name", "XY（床）", "XY (Floor)"),
    e!("sketch_ui.plane_front_name", "XZ（正面）", "XZ (Front)"),
    e!("sketch_ui.plane_side_name", "YZ（側面）", "YZ (Side)"),
    e!("sketch_ui.plane_onshape_name", "選んだ形の上", "On selected shape"),
    e!("sketch_ui.plane_height_name", "高さを指定", "Set height"),
    e!("sketch_ui.plane_camera_name", "カメラと平行", "Parallel to camera"),
    e!("sketch_ui.plane_custom_name", "任意の面", "Custom plane"),
    e!("sketch_ui.plane_floor_tip", "床（XY）にスケッチします。真上から見ます", "Sketch on the floor (XY). View from above"),
    e!("sketch_ui.plane_front_tip", "正面（XZ）にスケッチします。手前に押し出されます", "Sketch on the front (XZ). Extrudes toward you"),
    e!("sketch_ui.plane_side_tip", "側面（YZ）にスケッチします。右方向に押し出されます", "Sketch on the side (YZ). Extrudes toward the right"),
    e!("sketch_ui.plane_onshape_tip", "選んだ形の上面（水平な面）にスケッチします", "Sketch on a horizontal plane at the top of the selected shape"),
    e!("sketch_ui.plane_height_tip", "指定したZの高さの、水平なXY面にスケッチします", "Sketch on a horizontal XY plane at a set Z height"),
    e!(
        "sketch_ui.plane_camera_tip",
        "カメラの向きと平行な面を、カメラの中心を通るように固定します。もう一度クリックすると取り直します",
        "Fixes a plane parallel to the current camera view through the camera target. Click again to re-take it"
    ),
    e!(
        "sketch_ui.plane_custom_tip",
        "位置XYZと回転XYZで面を指定します。ほかの面から戻っても設定は残ります",
        "Custom plane set by position XYZ and rotation XYZ. Settings are kept when returning from other planes"
    ),
    e!("sketch_ui.pen_polygon", "多角形", "Polygon"),
    e!("sketch_ui.pen_rect", "長方形", "Rectangle"),
    e!("sketch_ui.pen_circle", "円", "Circle"),
    e!("sketch_ui.pen_spline", "スプライン", "Spline"),
    e!("sketch_ui.pen_line", "線", "Line"),
    e!("sketch_ui.pen_polygon_tip", "クリックで角を置き、最後に多角形を閉じてください", "Click to place corners, then close the polygon"),
    e!("sketch_ui.pen_rect_tip", "ドラッグ（または2回クリック）で長方形を描きます", "Drag (or click twice) to draw a rectangle"),
    e!("sketch_ui.pen_circle_tip", "中心から外へドラッグして円を描きます", "Drag from the center outward to draw a circle"),
    e!("sketch_ui.pen_spline_tip", "クリックした点を通る、なめらかな閉じた曲線を描きます", "Draw a smooth closed curve through the clicked points"),
    e!("sketch_ui.pen_line_tip", "スイープの経路に使う、開いた線を描きます", "Draw an open line to use as a sweep path"),
    e!("sketch_ui.make_extrude", "押し出し", "Extrude"),
    e!("sketch_ui.make_revolve", "回転", "Revolve"),
    e!("sketch_ui.make_loft", "ロフト", "Loft"),
    e!("sketch_ui.make_sweep", "スイープ", "Sweep"),
    e!("sketch_ui.op_trim", "トリム", "Trim"),
    e!("sketch_ui.op_offset", "オフセット", "Offset"),
    e!("sketch_ui.op_fillet", "フィレット", "Fillet"),
    e!("sketch_ui.msg_limit_points", "点は {0} 個までです", "Up to {0} points"),
    e!(
        "sketch_ui.msg_line_cross",
        "そこに線を引くと別の線と交差します。別の場所をクリックしてください",
        "A line there would cross another. Click a different spot"
    ),
    e!(
        "sketch_ui.msg_plane_from_camera",
        "カメラの向きから面を作れません。視点を調整してください",
        "Can't make a plane from the camera view. Adjust the view"
    ),
    e!(
        "sketch_ui.msg_plane_changed",
        "面を変えたため、描きかけの点は消えました。完成した輪郭と形は変わりません",
        "Changing plane cleared unfinished points. Finished outlines and solids are unchanged"
    ),
    e!("sketch_ui.msg_plane_invalid", "面の位置または角度が正しくありません", "Plane position or angle is invalid"),
    e!(
        "sketch_ui.msg_path_saved",
        "経路を {0} 点で保存しました。輪郭を描いてから、スイープを使ってください",
        "Path saved with {0} points. Draw an outline and use Sweep"
    ),
    e!("sketch_ui.msg_limit_shapes", "形は {0} 個までです", "Up to {0} shapes"),
    e!("sketch_ui.msg_holes_extrude_only", "穴のあるスケッチは、押し出しだけ使えます", "Sketches with holes can only be extruded"),
    e!("sketch_ui.msg_group_limit", "グループの数が上限に達しました", "Group limit reached"),
    e!("sketch_ui.msg_holes_failed", "穴の押し出しに失敗しました", "Failed to extrude holes"),
    e!("sketch_ui.hole_name", "穴 {0}", "Hole {0}"),
    e!("sketch_ui.group_extrude_holes", "穴あき押し出し", "Extrude with holes"),
    e!(
        "sketch_ui.msg_created",
        "{0} を作成しました（輪郭を変えるには「スケッチを編集」を使ってください）",
        "Created {0} (use Edit Sketch to change the outline)"
    ),
    e!(
        "sketch_ui.edit_outline_tip",
        "ビューで点を動かして、この形の輪郭を編集します",
        "Edit this shape's outline by moving its points in the viewport"
    ),
    e!("sketch_ui.finish_editing", "編集を完了", "Finish Editing"),
    e!("sketch_ui.edit_sketch", "スケッチを編集", "Edit Sketch"),
    e!(
        "sketch_ui.msg_offset_created",
        "オフセットの輪郭を作成しました。ソリッドにするか、Esc で破棄してください",
        "Offset outline created. Make it solid or press Esc to discard"
    ),
    e!("sketch_ui.msg_shape_not_found", "編集する形が見つかりません", "Shape to edit not found"),
    e!("sketch_ui.msg_limit_outline_points", "点は {0} 個までです", "Up to {0} points"),
    e!("sketch_ui.radius_mm_label", "半径 {0} mm", "Radius {0} mm"),
    e!("sketch_ui.done", "完了", "Done"),
    e!("sketch_ui.done_tip", "編集を完了（Enter / Esc）", "Finish editing (Enter / Esc)"),
    e!("sketch_ui.outline_bottom", "下側の輪郭", "Bottom outline"),
    e!("sketch_ui.outline_top", "上側の輪郭", "Top outline"),
    e!("sketch_ui.edit_label", "編集:", "Edit:"),
    e!("sketch_ui.offset_distance_label", "距離（mm、マイナスで内側）", "Distance (mm, negative = inward)"),
    e!("sketch_ui.radius_label", "半径（mm）", "Radius (mm)"),
    e!(
        "sketch_ui.trim_tip",
        "短い辺をクリックすると削除します。隣の辺が伸びて、輪郭は閉じたままになります",
        "Click a short edge to remove. Neighbor edges extend to keep the outline closed"
    ),
    e!(
        "sketch_ui.offset_tip",
        "緑のプレビューがオフセット結果を示します。元のソリッドは残ります",
        "A green preview shows the offset. The original solid is kept"
    ),
    e!("sketch_ui.fillet_tip", "角の頂点をクリックして丸めます。緑の円弧を確認してください", "Click a vertex to round. Check the green arc"),
    e!("sketch_ui.apply", "適用", "Apply"),
    e!("sketch_ui.cancel", "キャンセル", "Cancel"),
    e!(
        "sketch_ui.edit_help_outline",
        "点をドラッグ: 移動  ·  線をクリック: 点を追加  ·  点をクリックして Delete: 削除  ·  右クリック: 完了  ·  {0}/{1} 点",
        "Drag point: move  ·  Click line: add point  ·  Click point, Delete: remove  ·  Right-click: done  ·  {0}/{1} points"
    ),
    e!("sketch_ui.point_xy_label", "点の X / Y（mm）", "Point X / Y (mm)"),
    e!("sketch_ui.size_wh_label", "幅 / 高さ（mm）", "Width / Height (mm)"),
    e!("sketch_ui.diameter_label", "直径（mm）", "Diameter (mm)"),
    e!("sketch_ui.segment_length_label", "区間の長さ（mm）", "Segment length (mm)"),
    e!("sketch_ui.sketch_title", "スケッチ", "Sketch"),
    e!("sketch_ui.cancel_tip", "今のスケッチや操作を取り消します（Esc）", "Cancel the current sketch or operation (Esc)"),
    e!(
        "sketch_ui.sketch_hint",
        "左クリック: 点を置く  ·  右クリック: スケッチを完了  ·  Esc: 取り消し",
        "Left-click: place points  ·  Right-click: finish sketch  ·  Esc: cancel"
    ),
    e!("sketch_ui.pick_plane", "面を選ぶ", "Pick plane"),
    e!("sketch_ui.pick_plane_tip", "形の面か地面をクリックして、そこに描きます", "Click a face of a shape or the ground to draw on it"),
    e!("sketch_ui.more_planes", "ほかの面", "More planes"),
    e!("sketch_ui.more_planes_tip", "ほかの面: 選んだ形の上、高さ指定、任意", "More planes: on the selected shape, at a height, custom"),
    e!("sketch_ui.plane_head_on", "この面を正面から見る", "View this plane head-on"),
    e!("sketch_ui.plane_copy_custom", "この面を任意の面にコピー", "Copy this plane to custom plane"),
    e!(
        "sketch_ui.plane_draw_hint",
        "形の面か地面をクリックすると、そこから描き始めます",
        "Click a face of a shape, or the ground, to start drawing there"
    ),
    e!("sketch_ui.smooth_tip_on", "点を通るなめらかな曲線（クリックで直線に）", "Smooth curve through the points (click for straight lines)"),
    e!("sketch_ui.smooth_tip_off", "点の間を直線で結ぶ（クリックでなめらかな曲線に）", "Straight lines between points (click for a smooth curve)"),
    e!("sketch_ui.smooth", "なめらか", "Smooth"),
    e!("sketch_ui.sharp", "直線", "Straight"),
    e!(
        "sketch_ui.snap_tip",
        "角度を15°単位でスナップします。端点、中点、交点にもスナップします\nShift = 10 mm  ·  Ctrl = 0.1 mm\nBackspace: 直前の点を取り消す  ·  Esc: やり直す\nTab: 長さを入力",
        "Snap angles to 15°. Also snaps to endpoints, midpoints and crossings\nShift = 10 mm  ·  Ctrl = 0.1 mm\nBackspace: undo last point  ·  Esc: restart\nTab: type a length"
    ),
    e!("sketch_ui.angle_snap", "15°で角度スナップ", "Angle snap 15°"),
    e!("sketch_ui.holes_count", "穴: {0} / {1}", "Holes: {0} / {1}"),
    e!("sketch_ui.remove_last_hole", "最後の穴を削除", "Remove last hole"),
    e!("sketch_ui.finish_sketch", "スケッチを完了", "Finish Sketch"),
    e!(
        "sketch_ui.save_sketch_tip",
        "このスケッチを保存します（右クリック）。あとで選んで、押し出し、回転、ロフト、スイープに使えます",
        "Save this sketch (right-click). Select it afterwards for Extrude, Revolve, Loft or Sweep"
    ),
    e!("sketch_ui.redraw", "描き直す", "Redraw"),
    e!("sketch_ui.discard_draft_tip", "この下書きを捨てて、描き直します", "Discard this draft and draw again"),
    e!("sketch_ui.horizontal_z_label", "水平な面の高さ Z", "Horizontal plane height Z"),
    e!("sketch_ui.through_point_label", "通る点", "Through point"),
    e!("sketch_ui.rotation_label", "回転", "Rotation"),
    e!("sketch_ui.camera_target", "カメラの中心", "Camera target"),
    e!(
        "sketch_ui.camera_target_tip",
        "面の向きは保ったまま、カメラの中心を通します",
        "Keep this plane's orientation, pass through the camera target"
    ),
    e!("sketch_ui.shape_center", "選んだ形の中心", "Selected shape center"),
    e!(
        "sketch_ui.shape_center_tip",
        "面の向きは保ったまま、選んだ形の中心を通します",
        "Keep this plane's orientation, pass through the selected shape's center"
    ),
    e!(
        "sketch_ui.duplicate_custom_tip",
        "今の面を、動かしたり回転したりできる任意の面として複製します",
        "Duplicate the current plane as a custom plane that can be moved and rotated"
    ),
    e!(
        "sketch_ui.plane_fixed_hint",
        "カメラを回しても、この面は固定されたままです。向きを変えるには「カメラと平行」をもう一度押してください",
        "This plane stays fixed when the camera orbits. Press Parallel to camera again for a new orientation"
    ),
    e!("sketch_ui.reenter_dims", "寸法を入れ直す（Tab）", "Re-enter dimensions (Tab)"),
    e!("sketch_ui.tab_hint", "Tab: 次の欄  ·  Enter: 確定  ·  Esc: 取り消し", "Tab: next field  ·  Enter: confirm  ·  Esc: cancel"),
    e!("sketch_ui.create_label", "作成", "Create"),
    e!("sketch_ui.make_extrude_tip", "輪郭をまっすぐ押し出して、角柱にします", "Extrude the outline straight into a prism"),
    e!(
        "sketch_ui.make_revolve_tip",
        "輪郭を縦の中心軸のまわりに回転させて、花瓶のような形を作ります",
        "Revolve the outline around the vertical center axis to make a vase-like shape"
    ),
    e!(
        "sketch_ui.make_revolve_plane_tip",
        "回転は、正面または側面に描いたスケッチで使えます",
        "Revolve works on sketches drawn on the Front or Side plane"
    ),
    e!("sketch_ui.make_loft_tip", "下側の輪郭から上側の輪郭へ、なめらかにつなぎます", "Blend smoothly from the bottom outline to the top outline"),
    e!("sketch_ui.make_sweep_tip", "描いた経路に沿って輪郭を引き伸ばします", "Pull the outline along the drawn path"),
    e!("sketch_ui.height_label", "高さ", "Height"),
    e!("sketch_ui.flip", "反転", "Flip"),
    e!("sketch_ui.flip_tip", "面の反対側に押し出します", "Extrude to the opposite side of the plane"),
    e!("sketch_ui.both_sides", "両側", "Both sides"),
    e!("sketch_ui.scaled_copy", "拡大縮小コピー", "Scaled copy"),
    e!("sketch_ui.draw_another", "別の輪郭を描く", "Draw another"),
    e!("sketch_ui.draw_top", "上側の輪郭を描く", "Draw top outline"),
    e!("sketch_ui.redraw_top", "上側の輪郭を描き直す", "Redraw top outline"),
    e!("sketch_ui.top_scale_label", "上側の倍率", "Top scale"),
    e!("sketch_ui.top_scale_tip", "上側の輪郭の大きさ（比率、下側 = 1）", "Top outline size (ratio, bottom = 1)"),
    e!("sketch_ui.profile_scale_label", "輪郭の倍率", "Profile scale"),
    e!("sketch_ui.profile_scale_tip", "輪郭の大きさの倍率", "Outline size scale"),
    e!("sketch_ui.path_count", "経路: {0} 点", "Path: {0} points"),
    e!("sketch_ui.no_path", "経路はまだありません", "No path yet"),
    e!("sketch_ui.redraw_path", "経路を描き直す", "Redraw path"),
    e!("sketch_ui.draw_path", "経路を描く", "Draw path"),
    e!("sketch_ui.draw_path_tip", "経路を描きます（Enter で完了）", "Draw the path (Enter to finish)"),
    e!("sketch_ui.make_label", "{0} を作成", "Make {0}"),
    e!("sketch_ui.redo", "描き直す", "Redraw"),
    e!("sketch_ui.redo_tip", "輪郭を捨てて、描き直します（Esc）", "Discard the outline and draw again (Esc)"),
    e!(
        "sketch_ui.loft_top_hint",
        "ロフトの上側の輪郭を描いています（下側の輪郭は薄く表示されます）",
        "Drawing the loft's top outline (bottom outline shown faintly)"
    ),
    e!(
        "sketch_ui.polygon_hint",
        "左クリック: 角を置く  ·  右クリック / 最初の点 / Enter: 閉じる",
        "Left-click: place corner  ·  Right-click / first point / Enter: close"
    ),
    e!("sketch_ui.rect_hint", "ドラッグ（または2回クリック）で対角の角を指定します", "Drag (or click twice) to set opposite corners"),
    e!(
        "sketch_ui.circle_hint",
        "中心から外へドラッグ（または2回クリック）で半径を指定します",
        "Drag from center outward (or click twice) to set radius"
    ),
    e!(
        "sketch_ui.spline_hint",
        "左クリック: 点を通るなめらかな曲線  ·  右クリック / 最初の点 / Enter: 閉じる",
        "Left-click: smooth curve through points  ·  Right-click / first point / Enter: close"
    ),
    e!(
        "sketch_ui.sweep_path_hint",
        "スイープの経路を描いています  ·  左クリック: 点  ·  右クリック / Enter: 完了",
        "Drawing sweep path  ·  Left-click: point  ·  Right-click / Enter: finish"
    ),
    e!(
        "sketch_ui.line_hint",
        "左クリック: 点を置く  ·  右クリック / Enter: 完了（スイープの経路になります）",
        "Left-click: place point  ·  Right-click / Enter: finish (becomes sweep path)"
    ),
    e!("export_dialog.already_running", "書き出しはすでに実行中です", "Export is already running"),
    e!("export_dialog.cancelled", "キャンセルしました", "Cancelled"),
    e!(
        "export_dialog.too_big",
        "今の細かさでは大きすぎます。もう少し粗い設定を試してください",
        "Too big at this detail level. Try a coarser setting"
    ),
    e!("export_dialog.nothing", "書き出すものがありません", "Nothing to export"),
    e!("export_dialog.select_first", "先に形を選んでから書き出してください", "Select shapes first, then export"),
    e!("export_dialog.failed", "書き出しに失敗しました（{0}）", "Export failed ({0})"),
    e!("export_dialog.done", "書き出しました: {0}（三角形 {1} · {2} · {3} 秒）{4}", "Exported: {0} (triangles {1} · {2} · {3} sec){4}"),
    e!("export_dialog.exporting", "書き出し中… {0}", "Exporting… {0}"),
    e!("export_dialog.cancel", "キャンセル", "Cancel"),
    e!("export_dialog.title", "書き出し", "Export"),
    e!("export_dialog.intro", "3Dプリンターなどで使うファイルを作ります", "Makes a file for 3D printers and other software"),
    e!("export_dialog.format_stl_tip", "STL: 色はつきません。ほとんどの3Dプリンターで使えます", "STL: no colour. Works with most 3D printers"),
    e!("export_dialog.format_3mf_label", "色つき印刷", "Colour print"),
    e!("export_dialog.format_glb_label", "ほかのアプリ用", "Other apps"),
    e!("export_dialog.format_glb_tip", "GLB: Web や CG アプリ向け", "GLB: for web and CG apps"),
    e!("export_dialog.note_stl", "STL は色なしで保存されます", "STL is saved without colour"),
    e!("export_dialog.note_glb", "GLB は色と素材も保存されます", "GLB also keeps colour and material"),
    e!(
        "export_dialog.note_other",
        "別のファイル形式が選ばれています（詳細設定で変更できます）",
        "Another file type is selected (you can change it in advanced settings)"
    ),
    e!("export_dialog.detail_standard", "標準（おすすめ）", "Standard (recommended)"),
    e!("export_dialog.detail_standard_tip", "品質と時間のバランス", "Balance of quality and time"),
    e!("export_dialog.detail_fine", "細かい", "Fine"),
    e!("export_dialog.detail_fine_tip", "細かい形向け。時間がかかります", "For fine shapes. Takes longer"),
    e!("export_dialog.detail_fast", "速い / 軽い", "Fast / light"),
    e!("export_dialog.detail_fast_tip", "確認用。細い部分が消えることがあります", "For checking. Thin parts may disappear"),
    e!("export_dialog.detail_value", "細かさ {0} mm（詳細設定で変更）", "Detail {0} mm (change in advanced settings)"),
    e!("export_dialog.scope_visible", "見えている形", "Visible shapes"),
    e!("export_dialog.scope_selected", "選んだ形だけ", "Selected only"),
    e!("export_dialog.scope_all", "すべて", "All"),
    e!("export_dialog.scope_all_tip", "隠れている形も含みます", "Includes hidden shapes"),
    e!("export_dialog.need_selection", "先に形を選ぶか、「見えている形」に切り替えてください", "Select a shape first, or switch to 'Visible shapes'"),
    e!("export_dialog.too_fine", "細かすぎます。「速い / 軽い」を試してください", "Too fine. Try 'Fast / light'"),
    e!("export_dialog.estimate", "約 {0} · 最大 約 {1}", "About {0} · max about {1}"),
    e!("export_dialog.size_value", "大きさ: {0}", "Size: {0}"),
    e!("export_dialog.size_error", "大きさを確認できません。形を確認してください", "Couldn't check the size. Check the shape"),
    e!("export_dialog.size_checking", "大きさを確認中…", "Checking size…"),
    e!("export_dialog.hide_advanced", "詳細設定を隠す ▲", "Hide advanced ▲"),
    e!("export_dialog.show_advanced", "詳細設定を表示 ▼", "Show advanced ▼"),
    e!("export_dialog.file_type", "ファイル形式", "File type"),
    e!("export_dialog.type_stl_tip", "3Dプリント用（色なし・小さめ）", "For 3D printing (no colour, small)"),
    e!("export_dialog.type_3mf_tip", "スライサー用（色と単位つき）", "For slicers (colour and units)"),
    e!("export_dialog.type_obj_tip", "色つき（MTL ファイルも作ります）", "Coloured (also makes an MTL file)"),
    e!("export_dialog.type_ply_tip", "色つき（頂点の色）", "Coloured (vertex colour)"),
    e!("export_dialog.type_stl_ascii_tip", "テキスト形式の STL（ファイルが大きめ）", "Text STL (bigger file)"),
    e!("export_dialog.detail_by_number", "細かさを数値で指定", "Detail by number"),
    e!("export_dialog.cell_size_tip", "セルの大きさ (mm)。小さいほど細かいですが重くなります", "Cell size (mm). Smaller is finer but heavier"),
    e!("export_dialog.units", "単位", "Units"),
    e!("export_dialog.glb_units", "GLB は常に m 単位です", "GLB is always in m"),
    e!("export_dialog.size_label", "大きさ", "Size"),
    e!("export_dialog.up_direction", "上の向き", "Up direction"),
    e!("export_dialog.up_z", "Z（そのまま）", "Z (as is)"),
    e!("export_dialog.up_y", "Y（OBJ / GLB 風）", "Y (OBJ / GLB style)"),
    e!("export_dialog.up_y_tip", "Z 軸の上向きを Y 軸の上向きに変えます", "Turns Z-up into Y-up"),
    e!("export_dialog.smooth", "なめらかさ", "Smooth"),
    e!("export_dialog.smooth_none", "なし", "None"),
    e!("export_dialog.smooth_little", "少し", "A little"),
    e!("export_dialog.smooth_normal", "普通", "Normal"),
    e!("export_dialog.smooth_strong", "強め", "Strong"),
    e!("export_dialog.smooth_tip", "角はそのまま残ります", "Corners stay as they are"),
    e!("export_dialog.triangles", "三角形", "Triangles"),
    e!("export_dialog.tri_as_is", "そのまま", "As is"),
    e!("export_dialog.tri_as_is_tip", "すべてのセルを三角形にします", "All cells as triangles"),
    e!("export_dialog.tri_merge", "平らな面をまとめる", "Merge flat areas"),
    e!("export_dialog.tri_merge_tip", "平らな面を大きい三角形にします（形は変わりません）", "Flat faces into big triangles (shape stays the same)"),
    e!("export_dialog.tri_reduce", "数を減らす", "Reduce count"),
    e!("export_dialog.tri_reduce_tip", "三角形の数を決めて軽くします", "Set a triangle count to make it lighter"),
    e!("export_dialog.guide", "目安", "Guide"),
    e!("export_dialog.include_colour", "色を含める", "Include colour"),
    e!("export_dialog.ignore_paint", "塗った色を無視する", "Ignore painted colours"),
    e!("export_dialog.stl_no_colour", "STL は色を含められません", "STL can't include colour"),
    e!("export_dialog.triangles_about", "三角形 約 {0}", "Triangles, about {0}"),
    e!("export_dialog.print_check", "印刷チェック", "Print check"),
    e!("export_dialog.checking", "確認中…", "Checking…"),
    e!("export_dialog.export_button", "書き出し（{0}）…", "Export ({0})…"),
    e!("export_dialog.close", "閉じる", "Close"),
    e!("export_dialog.size_volume", "大きさ {0} ・ 体積 {1} cm³", "Size {0}  volume {1} cm³"),
    e!("export_dialog.surface_ok", "表面: 閉じています（印刷できます）", "Surface: closed (OK to print)"),
    e!(
        "export_dialog.surface_holes",
        "表面: 穴があります（形を重ねるか、溶かす強さを上げてください）",
        "Surface: has holes (overlap shapes or make the blend stronger)"
    ),
    e!(
        "export_dialog.thin_spots",
        "薄い部分: 約 {0}% が {1} mm{2} より薄いです（曲がったり、印刷に失敗することがあります）",
        "Thin spots: about {0}% is thinner than {1} mm{2} (may bend or fail to print)"
    ),
    e!("export_dialog.thin_none", "薄い部分: なし（{0} mm 以上）", "Thin spots: none (at least {0} mm)"),
    e!("export_dialog.overhang", "張り出し: 約 {0}% の面が 45° より傾いています（{1}）", "Overhang: about {0}% of faces tilt more than 45° ({1})"),
    e!("gpu_ui.report_failed", "状態: 失敗\n{0}\n", "Status: failed\n{0}\n"),
    e!("gpu_ui.report_preparing", "状態: 準備中\n", "Status: preparing\n"),
    e!("gpu_ui.report_done", "状態: 完了（{0} 秒）\n", "Status: done ({0} s)\n"),
    e!("gpu_ui.report_off", "状態: 使用していません\n", "Status: not in use\n"),
    e!("gpu_ui.report_error", "最後のエラー: {0}\n", "Last error: {0}\n"),
    e!("gpu_ui.open_failed", "開けませんでした: {0}", "Could not open: {0}"),
    e!(
        "gpu_ui.device_failed",
        "グラフィックを準備できませんでした（wgpu の描画情報がありません）。詳しくはログを見てください",
        "Could not create a graphics device (no wgpu render state). See the log for details."
    ),
    e!("gpu_ui.note_unsaved", "保存していない変更があります。先に保存してください（Ctrl+S）。", "You have unsaved changes. Save first (Ctrl+S)."),
    e!("gpu_ui.simple_view", "かんたん表示", "Simple view"),
    e!("gpu_ui.using", "使用中: {0}", "Using: {0}"),
    e!(
        "gpu_ui.meanwhile",
        "その間もかんたん表示で作業できます。準備ができたら自動で切り替わります",
        "Meanwhile you can work in Simple view. It switches over by itself when ready."
    ),
    e!("gpu_ui.too_long", "時間がかかるときは、下のボタンを試してください", "If this takes too long, try a button below."),
    e!("gpu_ui.try_again", "もう一度試す", "Try again"),
    e!("gpu_ui.try_again_tip", "グラフィックの準備をやり直します", "Start the graphics setup again"),
    e!("gpu_ui.restart_with_mode", "別のグラフィックモード（{0}）で再起動", "Restart with another graphics mode ({0})"),
    e!("gpu_ui.restart_auto", "別のグラフィックモードで再起動", "Restart with another graphics mode"),
    e!("gpu_ui.unsaved_save_first", "保存していない変更があります。先に保存してください（Ctrl+S）", "You have unsaved changes. Save first (Ctrl+S)"),
    e!(
        "gpu_ui.restart_tip",
        "アプリを再起動し、別のグラフィックモード（Vulkan、DirectX 12、OpenGL）を使います",
        "Restarts the app and uses another graphics mode (Vulkan, DirectX 12, OpenGL)"
    ),
    e!("gpu_ui.use_simple", "かんたん表示を使う", "Use Simple view"),
    e!("gpu_ui.use_simple_tip", "GPU を使わず、CPU で簡単に描きます", "Draw small on the CPU, without the GPU"),
    e!("gpu_ui.failed_title", "グラフィックの準備に失敗しました", "Graphics setup failed"),
    e!(
        "gpu_ui.failed_body",
        "今はかんたん表示で表示しています。このまま作業を続けられます",
        "You are seeing Simple view for now. You can keep working."
    ),
    e!("gpu_ui.driver", "ドライバー: {0}", "Driver: {0}"),
    e!("gpu_ui.error_details", "エラーの詳細（選んでコピーできます）", "Error details (select to copy)"),
    e!("gpu_ui.log_path", "ログ: {0}", "Log: {0}"),
    e!("gpu_ui.open_log", "ログを開く", "Open log"),
    e!("gpu_ui.copy_log", "ログをコピー", "Copy log"),
    e!(
        "gpu_ui.copy_log_tip",
        "エラーとログの最後の部分をコピーします。送るときに貼り付けてください",
        "Copies the error and the end of the log. Paste it when you send it."
    ),
    e!("gpu_ui.copied", "コピーしました", "Copied."),
    e!(
        "gpu_ui.unsaved_restart",
        "保存していない変更があります。先に保存してから再起動してください（Ctrl+S）",
        "You have unsaved changes. Save first, then restart (Ctrl+S)."
    ),
    e!("gpu_ui.simple_no_gpu_draw", "かんたん表示（GPU で描けません）", "Simple view (GPU drawing unavailable)"),
    e!("gpu_ui.details", "詳細", "Details"),
    e!("gpu_ui.simple_checkbox", "かんたん表示（GPU なし）", "Simple view (no GPU)"),
    e!(
        "gpu_ui.simple_checkbox_tip",
        "CPU で簡単に描きます。画面が真っ白のままか、描画が遅いときに使ってください",
        "Draws small on the CPU. Use it when the screen stays blank or drawing is slow."
    ),
    e!("gpu_ui.graphics_state", "グラフィック: {0}（{1}）", "Graphics: {0} ({1})"),
    e!("gpu_ui.see_failed", "失敗の内容を見る", "See what failed"),
    e!("gpu_ui.restart_menu", "別のグラフィックモードで再起動", "Restart with a different graphics mode"),
    e!("gpu_ui.save_first", "先に保存してください（Ctrl+S）", "Save first (Ctrl+S)"),
    e!("gpu_ui.auto_mode", "自動（おすすめ）", "Auto (recommended)"),
    e!("anim.max_poses", "ポーズは最大 {0} 個までです", "Up to {0} poses"),
    e!(
        "anim.max_tracks",
        "アニメーションできるのは {0} 個までです。{1} 個の形は記録されませんでした",
        "Only {0} shapes can be animated; {1} shape(s) were not recorded"
    ),
    e!("materials.category_basic", "基本", "Basic"),
    e!("materials.category_metal", "金属", "Metal"),
    e!("materials.category_glass", "ガラス", "Glass"),
    e!("materials.category_glow", "発光", "Glow"),
    e!("materials.category_wonder", "不思議", "Wonder"),
    e!("materials.mat_clay", "ねんど", "Clay"),
    e!("materials.mat_matte", "つや消し", "Matte"),
    e!("materials.mat_plastic", "プラスチック", "Plastic"),
    e!("materials.mat_glossy_plastic", "つやありプラスチック", "Glossy plastic"),
    e!("materials.mat_rubber", "ゴム", "Rubber"),
    e!("materials.mat_wax", "ロウ", "Wax"),
    e!("materials.mat_milk", "ミルク", "Milk"),
    e!("materials.mat_ink", "インク", "Ink"),
    e!("materials.mat_strawberry", "いちご", "Strawberry"),
    e!("materials.mat_mint", "ミント", "Mint"),
    e!("materials.mat_sky", "空色", "Sky"),
    e!("materials.mat_lavender", "ラベンダー", "Lavender"),
    e!("materials.mat_lemon", "レモン", "Lemon"),
    e!("materials.mat_pink", "ピンク", "Pink"),
    e!("materials.mat_nut", "ナッツ", "Nuts"),
    e!("materials.mat_stone", "石", "Stone"),
    e!("materials.mat_gold", "金", "Gold"),
    e!("materials.mat_rose_gold", "ローズゴールド", "Rose gold"),
    e!("materials.mat_silver", "銀", "Silver"),
    e!("materials.mat_chrome", "クローム", "Chrome"),
    e!("materials.mat_copper", "銅", "Copper"),
    e!("materials.mat_bronze", "ブロンズ", "Bronze"),
    e!("materials.mat_titanium", "チタン", "Titanium"),
    e!("materials.mat_rusty_iron", "さびた鉄", "Rusty iron"),
    e!("materials.mat_gunmetal", "ガンメタル", "Gunmetal"),
    e!("materials.mat_verdigris", "緑青", "Verdigris"),
    e!("materials.mat_clear_glass", "透明ガラス", "Clear glass"),
    e!("materials.mat_clear", "透明", "Clear"),
    e!("materials.mat_water", "水", "Water"),
    e!("materials.mat_frosted_glass", "すりガラス", "Frosted glass"),
    e!("materials.mat_blue_glass", "青いガラス", "Blue glass"),
    e!("materials.mat_green_glass", "緑のガラス", "Green glass"),
    e!("materials.mat_pink_glass", "ピンクのガラス", "Pink glass"),
    e!("materials.mat_ruby", "ルビー", "Ruby"),
    e!("materials.mat_amber", "琥珀", "Amber"),
    e!("materials.mat_diamond", "ダイヤモンド", "Diamond"),
    e!("materials.mat_drop", "しずく", "Drop"),
    e!("materials.mat_neon_pink", "ネオンピンク", "Neon pink"),
    e!("materials.mat_neon_blue", "ネオンブルー", "Neon blue"),
    e!("materials.mat_neon_green", "ネオングリーン", "Neon green"),
    e!("materials.mat_lamp", "ランプ", "Lamp"),
    e!("materials.mat_lava", "溶岩", "Lava"),
    e!("materials.mat_firefly", "ホタル", "Firefly"),
    e!("materials.mat_ice", "氷", "Ice"),
    e!("materials.mat_soap_bubble", "シャボン玉", "Soap bubble"),
    e!("materials.mat_hologram", "ホログラム", "Hologram"),
    e!("materials.mat_car_paint", "車の塗装", "Car paint"),
    e!("materials.mat_pearl", "パール", "Pearl"),
    e!("materials.mat_jewel_beetle", "玉虫", "Jewel beetle"),
    e!("materials.mat_candy", "キャンディ", "Candy"),
    e!("materials.mat_jelly", "ゼリー", "Jelly"),
    e!("materials.mat_opal", "オパール", "Opal"),
    e!("materials.mat_night_sky", "夜空", "Night sky"),
    e!("materials.mat_marshmallow", "マシュマロ", "Marshmallow"),
    e!("materials.mat_chocolate", "チョコレート", "Chocolate"),
    e!("materials.mat_sunset", "夕焼け", "Sunset"),
    e!("materials.mat_sea", "海", "Sea"),
    e!("materials.mat_ramune", "ラムネ", "Ramune"),
    e!("materials.mat_strawberry_milk", "いちごミルク", "Strawberry milk"),
    e!("materials.mat_aurora", "オーロラ", "Aurora"),
    e!("materials.mat_forest_morning", "森の朝", "Forest morning"),
    e!("materials.mat_mint_ice_cream", "ミントアイス", "Mint ice cream"),
    e!("materials.level_light", "軽い", "Light"),
    e!("materials.level_medium", "中くらい", "Medium"),
    e!("materials.level_heavy", "重い", "Heavy"),
    e!("svg_dialog.load_stopped", "SVG の読み込みが途中で止まりました", "The SVG load stopped partway through"),
    e!("svg_dialog.load_start_failed", "SVG の読み込みを開始できませんでした: {0}", "Couldn't start the SVG load: {0}"),
    e!("svg_dialog.outlines_skipped", "小さすぎる輪郭や余分な輪郭を {0} 個飛ばしました", "Skipped {0} tiny or extra outlines"),
    e!("svg_dialog.sample_star", "サンプルの星", "Sample star"),
    e!("svg_dialog.title", "SVG を読み込む", "Import SVG"),
    e!(
        "svg_dialog.solid_hint",
        "塗りつぶされた輪郭を立体にします。穴は開いたままになります。",
        "Turn filled outlines into a solid. Holes stay open."
    ),
    e!("svg_dialog.choose_button", "SVG を選ぶ…", "Choose SVG…"),
    e!("svg_dialog.sample_button", "サンプルを試す", "Try a sample"),
    e!("svg_dialog.drop_hint", "ウィンドウに .svg ファイルをドロップすることもできます。", "You can also drop a .svg file into the window."),
    e!("svg_dialog.stopping", "停止しています…", "Stopping…"),
    e!("svg_dialog.reading", "SVG を読み込んで確認しています…", "Reading and checking the SVG…"),
    e!("svg_dialog.building", "プレビューを作成しています…", "Building the preview…"),
    e!("svg_dialog.holes_hint", "赤い輪郭は穴を表します。", "Red outlines mark holes."),
    e!("svg_dialog.notes_heading", "読み込みメモ", "Import notes"),
    e!("svg_dialog.longest_side", "最長辺", "Longest side"),
    e!(
        "svg_dialog.longest_side_tip",
        "幅と高さのうち長い方です。縦横比は変わりません。",
        "Width or height, whichever is longer. The aspect ratio stays fixed."
    ),
    e!("svg_dialog.extrude_height", "押し出し高さ", "Extrude height"),
    e!("svg_dialog.round_edges", "角を丸める", "Round edges"),
    e!("svg_dialog.extrude_tip", "省略可。細かい部分を残すには 0 mm から始めてください。", "Optional. Start at 0 mm to preserve fine details."),
    e!("svg_dialog.original_size", "元の大きさ", "Original size"),
    e!("svg_dialog.flip_vertical", "上下を反転", "Flip vertically"),
    e!("svg_dialog.stop_loading", "読み込みを止める", "Stop loading"),
    e!("svg_dialog.import_button", "ソリッドを読み込む", "Import solid"),
    e!("svg_dialog.cancel_button", "キャンセル", "Cancel"),
    e!("svg_dialog.cancel_tip", "読み込みをキャンセル · Esc", "Cancel import · Esc"),
    e!("svg_dialog.choose_first", "輪郭をプレビューするには SVG を選んでください", "Choose an SVG to preview its outlines"),
    e!("svg_dialog.max_groups", "グループは {0} 個までです", "Up to {0} groups"),
    e!("svg_dialog.max_shapes", "形は {0} 個までです", "Up to {0} shapes"),
    e!("svg_dialog.no_shapes", "形を追加できませんでした", "No shapes could be added"),
    e!("svg_dialog.added", "SVG を追加しました（形 {0} 個）", "Added SVG ({0} shapes)"),
    e!("add_file.empty_file", "そのファイルは空です", "That file has nothing in it"),
    e!("add_file.too_many_names", "このファイルは安全に追加するには名前の数が多すぎます", "Too many names to add this file safely"),
    e!("add_file.max_animated", "動く形は {0} 個までです", "Up to {0} animated shapes"),
    e!("add_file.default_group_title", "追加したファイル", "Added file"),
    e!("add_file.no_group", "そのグループはありません", "No such group"),
    e!("add_file.not_a_piece", "このグループはファイルの一部ではありません", "This group is not a piece of a file"),
    e!(
        "add_file.piece_path_unknown",
        "この部品の元のファイルが分かりません。先に「ファイルとして保存…」を使ってください。",
        "Where this piece came from is not known. Use Save as file… first"
    ),
    e!("add_file.piece_of_file", "大きなファイルの一部: {0}", "Part of a large file: {0}"),
    e!(
        "add_file.whole_file_tip",
        "追加した1つのファイルです。このシーンの他の部品や形と溶かして一体化します。",
        "A whole file you added. It melts with the other pieces and shapes of this scene"
    ),
    e!("add_file.reload_button", "再読み込み", "Reload"),
    e!("add_file.reload_tip", "ファイルを読み直して、この部品をその内容に置き換えます", "Read the file again and replace this piece with it"),
    e!("add_file.reloaded", "ファイルを読み直しました", "Read the file again"),
    e!("add_file.reload_failed", "再読み込みできませんでした: {0}", "Could not reload: {0}"),
    e!("add_file.save_button", "ファイルとして保存…", "Save as file…"),
    e!("add_file.save_tip", "この部品を単独のファイルに書き出します", "Write this piece into a file of its own"),
    e!("add_file.saved", "部品をファイルとして保存しました", "Saved the piece as a file"),
    e!("add_file.save_failed", "保存できませんでした: {0}", "Could not save: {0}"),
    e!("add_file.added", "ファイルを追加しました。今あるものと溶かして一体化しました。", "Added the file. It melts with what was already here"),
    e!("add_file.add_failed", "ファイルを追加できませんでした: {0}", "Could not add the file: {0}"),
    e!("cpu_render.stopped", "停止しました", "Stopped"),
    e!("cpu_render.no_poses", "ポーズがまだありません。先にポーズを設定してください。", "No poses yet. Set a pose first"),
    e!("cpu_render.missing_frame", "コマが見つかりません", "Missing frame"),
    e!("cpu_render.bad_size", "画像のサイズが正しくありません", "Invalid image size"),
    e!("palette.cmd_add_item", "{0} を追加", "Add {0}"),
    e!("palette.tag_shape", "形", "Shape"),
    e!("palette.tag_library", "ライブラリ", "Library"),
    e!("palette.cmd_ripple_concentric", "リップル: 同心円", "Ripple: Concentric"),
    e!("palette.cmd_ripple_linear", "リップル: 直線", "Ripple: Linear"),
    e!("palette.cmd_ripple_random", "リップル: ランダム", "Ripple: Random"),
    e!("palette.cmd_melt_preset", "溶かす (たれる形)", "Melt (dripping shape)"),
    e!("palette.cmd_group", "グループ", "Group"),
    e!("palette.cmd_undo", "元に戻す", "Undo"),
    e!("palette.cmd_redo", "やり直す", "Redo"),
    e!("palette.cmd_copy", "コピー", "Copy"),
    e!("palette.cmd_cut", "切り取り", "Cut"),
    e!("palette.cmd_paste", "貼り付け", "Paste"),
    e!("palette.cmd_duplicate", "複製", "Duplicate"),
    e!("palette.cmd_mirror_copy", "左右コピー", "Mirror Copy"),
    e!("palette.cmd_delete", "削除", "Delete"),
    e!("palette.cmd_array", "並べてコピー", "Array"),
    e!("palette.cmd_pattern_rect", "パターン: 四角形状", "Pattern: Rectangular"),
    e!("palette.cmd_pattern_circular", "パターン: 円形状", "Pattern: Circular"),
    e!("palette.cmd_pattern_path", "パターン: 線に沿って", "Pattern: Along Path"),
    e!("palette.cmd_pattern_points", "パターン: 点ごと", "Pattern: Points"),
    e!("palette.cmd_mirror_x", "ミラー X (左右に写す)", "Mirror X"),
    e!("palette.cmd_mirror_y", "ミラー Y (前後に写す)", "Mirror Y"),
    e!("palette.cmd_mirror_z", "ミラー Z (上下に写す)", "Mirror Z"),
    e!("palette.cmd_clear_pattern", "パターンを解除", "Clear Pattern"),
    e!("palette.cmd_make_instance", "リンクコピーを作る", "Make Linked Copy"),
    e!("palette.cmd_new_scene", "新しいシーン", "New Scene"),
    e!("palette.cmd_open", "開く", "Open"),
    e!("palette.cmd_save", "保存", "Save"),
    e!("palette.cmd_save_as", "名前を付けて保存", "Save As"),
    e!("palette.cmd_export_stl", "STL で書き出す", "Export STL"),
    e!("palette.cmd_export_3mf", "3MF で書き出す", "Export 3MF"),
    e!("palette.cmd_export_obj", "OBJ で書き出す", "Export OBJ"),
    e!("palette.cmd_export_png", "PNG 画像で書き出す", "Export PNG"),
    e!("palette.cmd_print_check", "印刷チェック (大きさ・体積)", "Print Check"),
    e!("palette.cmd_import_svg", "SVG を読み込む", "Import SVG"),
    e!("palette.cmd_organic_blob", "ぷっくり形 (有機的な形)", "Organic Blob"),
    e!("palette.cmd_import_mesh", "メッシュを読み込む", "Import Mesh"),
    e!("palette.cmd_frame_all", "全体を表示", "Frame All"),
    e!("palette.cmd_frame_selection", "選んだものを表示", "Frame Selection"),
    e!("palette.cmd_toggle_perspective", "遠近 / 平行投影を切り替え", "Toggle Perspective / Ortho"),
    e!("palette.cmd_toggle_add_subtract", "足す / 削るを切り替え", "Toggle Add / Subtract"),
    e!("palette.cmd_next_add", "次は足す", "Next: Add"),
    e!("palette.cmd_next_subtract", "次は削る", "Next: Subtract"),
    e!("palette.cmd_next_paint", "次は塗る", "Next: Paint"),
    e!("palette.cmd_next_intersect", "次は重ねる", "Next: Intersect"),
    e!("palette.cmd_tool_select", "道具: 選ぶ", "Tool: Select"),
    e!("palette.cmd_tool_all", "道具: すべて", "Tool: All"),
    e!("palette.cmd_tool_move", "道具: 動かす", "Tool: Move"),
    e!("palette.cmd_tool_rotate", "道具: 回す", "Tool: Rotate"),
    e!("palette.cmd_tool_scale", "道具: 大きさを変える", "Tool: Scale"),
    e!("palette.cmd_view_front", "正面から見る", "View Front"),
    e!("palette.cmd_view_right", "右から見る", "View Right"),
    e!("palette.cmd_view_top", "真上から見る", "View Top"),
    e!("palette.cmd_show_library", "ライブラリを開く", "Show Library"),
    e!("palette.cmd_show_outliner", "シーン一覧を開く", "Show Outliner"),
    e!("palette.cmd_keyboard_shortcuts", "キーボード操作の一覧", "Keyboard Shortcuts"),
    e!("palette.cmd_first_box_guide", "はじめての箱づくりガイド", "First Box Guide"),
    e!("palette.cmd_restore_hints", "隠したヒントを元に戻す", "Restore Hidden Hints"),
    e!("palette.model_not_loaded", "このモデルは読み込めませんでした", "This model could not be loaded"),
    e!("palette.set_shortcut", "ショートカットを設定: {0}", "Set shortcut: {0}"),
    e!("palette.press_key_combo", "キーの組み合わせを押してください (Esc でキャンセル)", "Press a key combo (Esc to cancel)"),
    e!("palette.clear_shortcut", "消去", "Clear"),
    e!("palette.cancel", "キャンセル", "Cancel"),
    e!("palette.search_hint", "形・ライブラリ・コマンドを検索 (日本語でも検索できます)", "Search shapes, library, commands (Japanese also works)"),
    e!("palette.run_tip", "実行 (右クリックでピン留めを解除)", "Run (right-click to unpin)"),
    e!("palette.no_results", "結果がありません", "No results"),
    e!("palette.unpin", "ピン留めを解除", "Unpin"),
    e!("palette.pin_top", "一番上にピン留め", "Pin to the top"),
    e!("palette.make_organic_tube", "Organic Tube・{0}を作る", "Make Organic Tube・{0}"),
    e!("palette.preset_tag", "プリセット", "Preset"),
    e!("palette.open_solidcraft_label", "SolidCraft で精密に作る", "Model precisely in SolidCraft"),
    e!("model.blend_smooth", "なめらか", "Smooth"),
    e!("model.blend_round", "丸く", "Round"),
    e!("model.blend_gooey", "ねばねば", "Gooey"),
    e!(
        "model.too_many_groups",
        "このプロジェクトにはグループが {0} 個あります (上限は {1} 個)。何も読み込まず、元のファイルは残しました。",
        "This project contains {0} groups (maximum {1}). Nothing was loaded; the original file was preserved."
    ),
    e!(
        "model.too_many_shapes_short",
        "このプロジェクトは形が多すぎます。元のファイルは残しました",
        "This project has too many shapes; the original file was preserved"
    ),
    e!(
        "model.too_many_shapes",
        "このプロジェクトには形が {0} 個あります (上限は {1} 個)。何も読み込まず、元のファイルは残しました。",
        "This project contains {0} shapes (maximum {1}). Nothing was loaded; the original file was preserved."
    ),
    e!("model.err_no_editable_outline", "この形には編集できる輪かくがありません", "This shape has no editable outline"),
    e!("model.err_outline_coord_range", "輪かくの座標は±2000mm以内にしてください", "Keep outline coordinates within ±2000 mm"),
    e!("model.preset_studio", "スタジオ", "Studio"),
    e!("model.preset_daytime", "ひるま", "Daytime"),
    e!("model.preset_sunset", "ゆうやけ", "Sunset"),
    e!("model.preset_night", "よる", "Night"),
    e!("model.preset_candy", "キャンディ", "Candy"),
    e!("model.preset_white", "しろ", "White"),
    e!("model.err_serialize", "作品のシリアライズに失敗しました", "Couldn't save the project data"),
    e!(
        "model.note_broken_meshes_removed",
        "こわれていて読めないメッシュが{0}個あったので、取りのぞきました",
        "Removed {0} broken meshes that could not be read"
    ),
    e!(
        "model.note_too_many_meshes",
        "メッシュが多すぎるので、{0}個を取りのぞきました（{1}個・データ量に上限があります）",
        "Too many meshes, so {0} were removed (data size is limited to {1})"
    ),
    e!(
        "model.note_furniture_changed",
        "家具プリセットの部品が変わった{0}組は、形を残して通常のグループとして開きました",
        "{0} furniture sets had changed parts, so they were opened as regular groups with their shapes kept"
    ),
    e!("import_mesh.face_too_many_verts", "面の頂点が多すぎます", "A face has too many vertices"),
    e!("import_mesh.face_too_few_verts", "面の頂点が 3 個より少ないです", "A face has fewer than 3 vertices"),
    e!("import_mesh.ply_props_missing", "行を持つ PLY の要素には、プロパティを指定してください", "A PLY element with rows must declare properties"),
    e!("import_mesh.ply_too_many_elements", "PLY の要素が多すぎます", "Too many elements in the PLY"),
    e!("import_mesh.ply_no_xyz", "PLY に x y z の座標がありません", "The PLY has no x y z coordinates"),
    e!("import_mesh.ply_face_count", "PLY の面の頂点数が正しくありません", "The PLY face vertex count is wrong"),
    e!(
        "import_mesh.obj_mtl_not_read",
        "OBJ の素材ファイル (.mtl) は読み込まないので、色と素材は残りません",
        "The OBJ's material file (.mtl) is not read, so its colours and materials are not kept"
    ),
    e!(
        "import_mesh.ply_vertex_colours",
        "PLY の頂点の色は残らず、形は 1 色になります",
        "The PLY's vertex colours are not kept; the shape gets one colour"
    ),
    e!(
        "import_mesh.format_unknown",
        "このファイルの形式がわかりません (対応: STL, OBJ, PLY, 3MF, GLB)",
        "Can't tell this file's format (supports STL, OBJ, PLY, 3MF, GLB)"
    ),
    e!(
        "import_mesh.several_colours",
        "複数の色や素材があります。SDF は面ごとの色を残せないため、メインの色を 1 つ使います (元の色の分け方は保存されません)",
        "Several colours or materials found. SDF can't keep colours per face, so one main colour is used (the original colour split is not saved)"
    ),
    e!(
        "import_mesh.holes",
        "穴があります (開いた辺: {0} 本)。形はおおよそで読み込まれます",
        "There are holes (open edges: {0}). The shape is loaded approximately"
    ),
    e!("import_mesh.nonmanifold_edges", "3 つ以上の面が集まっている辺があります ({0} 本)", "Some edges have 3 or more faces meeting ({0} edges)"),
    e!(
        "import_mesh.inconsistent_winding",
        "向きが逆になっている面があります ({0} 本の辺)。内側と外側が入れ替わることがあります",
        "Some faces are turned the wrong way ({0} edges). Inside and outside may be mixed up"
    ),
    e!("import_mesh.removed_zero_area", "面積ゼロの三角形を {0} 個取り除きました", "Removed {0} zero-area triangles"),
    e!("import_mesh.flat_shape", "平らな形です (厚みがありません)", "Flat shape (no thickness)"),
    e!("import_mesh.open_error", "ファイルを開けません: {0}", "Can't open the file: {0}"),
    e!("import_mesh.read_error", "ファイルを読めません: {0}", "Can't read the file: {0}"),
    e!("palette_prefs.undo", "元に戻す", "Undo"),
    e!("palette_prefs.redo", "やり直す", "Redo"),
    e!("palette_prefs.copy", "コピー", "Copy"),
    e!("palette_prefs.cut", "切り取り", "Cut"),
    e!("palette_prefs.paste", "貼り付け", "Paste"),
    e!("palette_prefs.save", "保存", "Save"),
    e!("palette_prefs.open", "開く", "Open"),
    e!("palette_prefs.export", "書き出し", "Export"),
    e!("palette_prefs.duplicate", "複製", "Duplicate"),
    e!("palette_prefs.group", "グループ", "Group"),
    e!("palette_prefs.find", "検索", "Find"),
    e!("palette_prefs.delete", "削除", "Delete"),
    e!("palette_prefs.help", "ヘルプ", "Help"),
    e!("palette_prefs.performance", "パフォーマンス", "Performance"),
    e!("palette_prefs.add_subtract", "追加 / 解除", "Add / Subtract"),
    e!("palette_prefs.frame_selection", "選択範囲を表示", "Frame selection"),
    e!("palette_prefs.frame_all", "すべてを表示", "Frame all"),
    e!("palette_prefs.confirm", "確定", "Confirm"),
    e!("palette_prefs.front_view", "正面ビュー", "Front view"),
    e!("palette_prefs.right_view", "右ビュー", "Right view"),
    e!("palette_prefs.perspective", "透視図", "Perspective"),
    e!("palette_prefs.top_view", "上ビュー", "Top view"),
    e!("palette_prefs.back_view", "背面ビュー", "Back view"),
    e!("palette_prefs.left_view", "左ビュー", "Left view"),
    e!("palette_prefs.bottom_view", "下ビュー", "Bottom view"),
    e!("palette_prefs.turn_view", "視点を回転", "Turn view"),
    e!("palette_prefs.nudge", "少しずつ動かす", "Nudge"),
    e!("sketch.too_few_points", "点を 3 つ以上置いてください", "Needs at least 3 points"),
    e!("sketch.bad_number", "数値が正しくありません。描き直してください", "Invalid number. Please redraw"),
    e!("sketch.too_many_points", "点が多すぎます（最大 {0} 個まで）", "Too many points (up to {0})"),
    e!("sketch.too_small", "小さすぎます（ほぼ線です）。もっと大きな形を描いてください", "Too small (almost a line). Draw a larger shape"),
    e!("sketch.self_intersect", "線が交差しています。交差しないように描き直してください", "Lines cross. Redraw without crossings"),
    e!("sketch.hole_outside", "穴の輪郭は外側の輪郭の内側に置いてください", "Hole outlines must be inside the outer outline"),
    e!(
        "sketch.hole_overlap",
        "穴が重なっています。離してください（穴の中に穴は作れません）",
        "Holes overlap. Separate them (holes cannot contain holes)"
    ),
    e!(
        "sketch.contours_touch",
        "輪郭が交差しているか接しています。外側の輪郭と穴は 0.05 mm 以上離してください",
        "Outlines cross or touch. Keep the outer edge and holes at least 0.05 mm apart"
    ),
    e!("sketch.too_many_holes", "穴は最大 {0} 個までです", "Up to {0} holes"),
    e!(
        "sketch.hole_mismatch",
        "穴と外側の輪郭で高さまたは向きが合いません。同じ面と押し出しの高さを使ってください",
        "Hole and outer outline don't match in height or orientation. Use the same plane and extrude height"
    ),
    e!(
        "sketch.left_of_axis",
        "左側で中心軸を越えています。中心軸の右側に描いてください",
        "Extends past the center axis on the left. Draw on the right side"
    ),
    e!("sketch.needs_vertical_plane", "回転には正面または側面の面にスケッチが必要です", "Revolve needs a sketch on the Front or Side plane"),
    e!("sketch.path_too_short", "パスが短すぎます。2 点以上を離して置いてください", "Path too short. Place at least 2 points, spaced apart"),
    e!("sketch.origin", "原点", "Origin"),
    e!("sketch.endpoint", "端点", "Endpoint"),
    e!("sketch.intersection", "交点", "Intersection"),
    e!("sketch.center", "中心", "Center"),
    e!("sketch.midpoint", "中点", "Midpoint"),
    e!("sketch.horizontal", "水平", "Horizontal"),
    e!("sketch.vertical", "垂直", "Vertical"),
    e!("sketch.grid", "グリッド", "Grid"),
    e!("sketch_dimensions.width", "幅", "Width"),
    e!("sketch_dimensions.height", "高さ", "Height"),
    e!("sketch_dimensions.radius", "半径", "Radius"),
    e!("sketch_dimensions.diameter", "直径", "Diameter"),
    e!("sketch_dimensions.length", "長さ", "Length"),
    e!("sketch_dimensions.angle", "角度", "Angle"),
    e!("sketch_dimensions.field_not_found", "入力欄が見つかりません", "Input field not found"),
    e!("sketch_dimensions.too_long", "32 文字以内の数値を入力してください", "Enter a number of 32 characters or fewer"),
    e!("sketch_dimensions.not_a_number", "正しい数値を入力してください", "Enter a valid number"),
    e!("sketch_dimensions.not_finite", "数値を入力してください（無限大は使えません）", "Enter a number (infinity is not allowed)"),
    e!("sketch_dimensions.rect_too_small", "幅と高さは 0.5 mm より大きくしてください", "Width and height must be greater than 0.5 mm"),
    e!("sketch_dimensions.coord_outside", "座標がスケッチの範囲外です", "Coordinate is outside the sketch area"),
    e!("sketch_dimensions.radius_too_small", "半径は 0.5 mm より大きくしてください", "Radius must be greater than 0.5 mm"),
    e!("sketch_dimensions.circle_orientation", "円の向きを決められません", "Can't determine circle orientation"),
    e!("sketch_dimensions.circle_outside", "円がスケッチの範囲外です", "Circle is outside the sketch area"),
    e!(
        "sketch_dimensions.segment_range",
        "長さは 0.5 mm より大きく、角度は ±36000° 以内にしてください",
        "Length must be greater than 0.5 mm and angle within ±36000°"
    ),
    e!("sketch_dimensions.point_outside", "点がスケッチの範囲外です", "Point is outside the sketch area"),
    e!("sketch_dimensions.crosses_here", "この点で輪郭が交差しています", "The outline crosses at this point"),
    e!("sketch_dimensions.xy_range", "X と Y は ±2000 mm 以内にしてください", "X and Y must be within ±2000 mm"),
    e!("sketch_dimensions.too_close", "前の点から離して置いてください", "Place it apart from the previous point"),
    e!("sketch_dimensions.fill_sizes", "大きさごとに数値を入力してください", "Type a number for each size"),
    e!("precise_services.filter_all", "デザイン、CADファイル、メッシュ", "Designs, CAD files and meshes"),
    e!("precise_services.fcstd", "FCStd", "FCStd"),
    e!("precise_services.filter_design", "SolidCraft のデザイン（コンテナーまたは JSON）", "SolidCraft design (container or JSON)"),
    e!("precise_services.filter_cad", "FreeCAD と Rhino", "FreeCAD and Rhino"),
    e!("precise_services.filter_mesh", "メッシュ (3MF, STL, OBJ, glTF)", "Mesh (3MF, STL, OBJ, glTF)"),
    e!("precise_services.filter_sketch", "スケッチ図面 (DXF, SVG)", "Sketch drawings (DXF, SVG)"),
    e!("precise_services.filter_any", "すべてのファイル", "All files"),
    e!("precise_services.choose_slicer", "スライサーを選んでください", "Choose your slicer"),
    e!("precise_services.programs", "プログラム", "Programs"),
    e!("precise_services.filter_json", "JSON 形式のデザイン", "JSON design"),
    e!("precise_services.filter_container", "SolidCraft コンテナー", "SolidCraft container"),
    e!("precise_services.file", "ファイル", "File"),
    e!("mesh_dialog.stopped_partway", "途中で中止しました", "Stopped partway through"),
    e!("mesh_dialog.load_start_failed", "読み込みを開始できませんでした: {0}", "Couldn't start loading: {0}"),
    e!(
        "mesh_dialog.original_changed",
        "元のファイルが変わっているため、別のモデルで上書きできません",
        "The original file has changed. Can't overwrite with a different model"
    ),
    e!(
        "mesh_dialog.old_mesh_no_source",
        "この古いメッシュは元の読み込み元が分かりません。元のメッシュをもう一度読み込んでください",
        "This old mesh has no identifiable source. Please import the original mesh again"
    ),
    e!(
        "mesh_dialog.name_mismatch_pick",
        "ファイル名が元のメッシュと一致しません。元のファイルを選んでください",
        "File name doesn't match the original mesh. Pick the original file"
    ),
    e!(
        "mesh_dialog.name_mismatch_short",
        "ファイル名が一致しません。元のファイルを選んでください",
        "File name doesn't match. Pick the original file"
    ),
    e!("mesh_dialog.pick_again_msg", "元のメッシュファイルをもう一度選んでください", "Please pick the original mesh file again"),
    e!("mesh_dialog.target_changed", "編集中に対象が変わりました。もう一度実行してください", "The target changed while editing. Please run it again"),
    e!("mesh_dialog.rebake_title", "メッシュを再計算", "Re-bake mesh"),
    e!("mesh_dialog.no_distance_data", "距離データがありません", "No distance data"),
    e!("mesh_dialog.actual_cell_width", "現在の実測セル幅: X {0} / Y {1} / Z {2} mm", "Current actual cell width: X {0} / Y {1} / Z {2} mm"),
    e!("mesh_dialog.target_label", "対象: {0}", "Target: {0}"),
    e!("mesh_dialog.detail_colon", "細かさ:", "Detail:"),
    e!(
        "mesh_dialog.estimate_nodes",
        "見積もり点数: {0} / 上限 {1}、距離データ用の作業配列は約 {2} MB",
        "Estimated nodes: {0} / limit {1}, working distance array about {2} MB"
    ),
    e!(
        "mesh_dialog.thin_parts_note",
        "注意: 細い部分はセル幅に近いため、消えることがあります",
        "Note: thin parts are close to the cell width and may disappear"
    ),
    e!(
        "mesh_dialog.over_limit",
        "メモリまたはシーンの上限を超えています。細かさを下げてください",
        "Over the memory or scene limit. Lower the detail"
    ),
    e!("mesh_dialog.original_file_path", "元のファイル: {0}", "Original file: {0}"),
    e!(
        "mesh_dialog.original_location_unknown",
        "元のファイルの場所が分かりません。もう一度選んでください",
        "Original file location unknown. Please pick it again"
    ),
    e!("mesh_dialog.recalc_progress", "再計算中 {0}% / {1} 秒", "Recalculating {0}% / {1} sec"),
    e!("mesh_dialog.stop_button", "中止", "Stop"),
    e!("mesh_dialog.pick_again_button", "元のファイルを選び直す…", "Pick original file again…"),
    e!("mesh_dialog.start_rebake", "再計算を開始", "Start re-bake"),
    e!("mesh_dialog.close_button", "閉じる", "Close"),
    e!(
        "mesh_dialog.busy_open",
        "処理中です。現在の処理を中止し、終わるまで待ってから別のファイルを開いてください",
        "Busy. Stop the current job and wait for it to end before opening another file"
    ),
    e!("mesh_dialog.cancelled", "キャンセルされました", "Cancelled"),
    e!("mesh_dialog.max_groups", "グループは {0} 個までです", "Up to {0} groups"),
    e!("mesh_dialog.max_shapes", "形は {0} 個までです", "Up to {0} shapes"),
    e!("mesh_dialog.mesh_added", "メッシュを追加しました: {0}（{1} · {2} 秒）", "Added mesh: {0} ({1} · {2} sec)"),
    e!("mesh_dialog.import_title", "メッシュを読み込む", "Import mesh"),
    e!(
        "mesh_dialog.import_desc",
        "STL、OBJ、3MF、PLY、GLB のソリッドを読み込みます。ほかの形と同じように溶かしたり切ったりできます",
        "Loads STL, OBJ, 3MF, PLY and GLB solids. Melt or cut them like other shapes"
    ),
    e!("mesh_dialog.reading_file", "ファイルを読み込んでいます…", "Reading the file…"),
    e!(
        "mesh_dialog.waiting_stop_memory",
        "中止が終わるのを待っています（解析後にメモリを解放します）…",
        "Waiting for the stop to finish (memory is freed after parsing)…"
    ),
    e!("mesh_dialog.waiting_stop", "中止が終わるのを待っています…", "Waiting for the stop to finish…"),
    e!("mesh_dialog.load_button", "読み込む", "Load"),
    e!("mesh_dialog.cancel_button", "キャンセル", "Cancel"),
    e!("mesh_dialog.measuring_shape", "形を測定しています… {0}", "Measuring the shape… {0}"),
    e!("mesh_dialog.calc_distance", "周囲の空間までの距離を計算しています", "Calculating distance to the surrounding space"),
    e!("mesh_dialog.choose_file", "ファイルを選ぶ…", "Choose file…"),
    e!("mesh_dialog.original_size_value", "元の大きさ {0}", "Original size {0}"),
    e!("mesh_dialog.target_size", "目標の大きさ", "Target size"),
    e!("mesh_dialog.longest_side_hint", "一番長い辺の長さ", "Length of the longest side"),
    e!("mesh_dialog.original_size_button", "元の大きさ", "Original size"),
    e!(
        "mesh_dialog.original_size_hint",
        "ファイルの大きさのままにします（200 mm より大きい場合は 200 mm まで）",
        "Keeps the file's size (capped at 200 mm if bigger)"
    ),
    e!("mesh_dialog.orientation", "向き", "Orientation"),
    e!("mesh_dialog.place_on_floor", "床に置く", "Place on floor"),
    e!("mesh_dialog.place_on_floor_hint", "底を床（高さ 0）に揃えます", "Lines the bottom up with the floor (height 0)"),
    e!("mesh_dialog.centre_it", "中央に置く", "Centre it"),
    e!(
        "mesh_dialog.centre_hint",
        "表示の中央に置きます。オフの場合はファイルの位置のままにします",
        "Puts it in the middle of the view. Off keeps the file's position"
    ),
    e!("mesh_dialog.detail_label", "細かさ", "Detail"),
    e!(
        "mesh_dialog.detail_hint",
        "一番長い辺にそろえるセルの数。大きいほどなめらかになりますが、重くなります",
        "Cells across the longest side. Bigger is smoother but heavier"
    ),
    e!("mesh_dialog.estimate_summary", "約 {0} · ファイル {1} · セル {2}", "About {0} · file {1} · cells {2}"),
    e!("mesh_dialog.original_file_label", "元のファイル", "Original file"),
    e!("mesh_dialog.size_label", "大きさ", "Size"),
    e!(
        "mesh_dialog.saved_cell_info",
        "保存時のセル {0} mm / 実測セル X {1} Y {2} Z {3} mm  {4}×{5}×{6}",
        "Saved cell {0} mm / actual cell X {1} Y {2} Z {3} mm  {4}×{5}×{6}"
    ),
    e!("mesh_dialog.no_data", "データがありません", "No data"),
    e!("mesh_dialog.raise_resolution", "解像度を上げて再計算…", "Raise resolution / re-bake…"),
    e!("mesh_dialog.restore_size", "元の大きさに戻す", "Restore original size"),
    e!("mesh_dialog.cancelled_import", "SolidCraft の取り込みを取り消しました", "SolidCraft import cancelled"),
    e!(
        "mesh_dialog.finish_first",
        "先にメッシュの取り込みや再計算を完了するか、取り消してください",
        "Finish or cancel the mesh import or recalculation first"
    ),
    e!(
        "mesh_dialog.cancelled_switch_mesh",
        "別のメッシュの読み込みに切り替えたため、SolidCraft の取り込みを取り消しました",
        "Switched to loading another mesh, so the SolidCraft import was cancelled"
    ),
    e!(
        "mesh_dialog.finish_sketch_first",
        "編集中のスケッチや立体を確定するか、取り消してから取り込んでください",
        "Finish or cancel the sketch or solid being edited, then import"
    ),
    e!("mesh_dialog.imported_solid", "SolidCraft の立体を取り込みました（{0}・{1} 秒）", "Imported SolidCraft solid ({0} · {1} sec)"),
    e!("mesh_dialog.importing_title", "SolidCraft の立体を Melt に取り込み中", "Importing SolidCraft solid into Melt"),
    e!("mesh_dialog.importing_note", "大きさと位置を保って、メルトできる形に変換します。", "Converts to a meltable shape, keeping size and position"),
    e!("mesh_dialog.waiting", "前の作業が終わるのを待っています…", "Waiting for the previous task to finish…"),
    e!("mesh_dialog.solid_size", "立体の大きさ: {0}", "Solid size: {0}"),
    e!(
        "mesh_dialog.work_remains",
        "SolidCraft の作業は残っています。もう一度試すか、取り消して編集に戻れます。",
        "SolidCraft work is still there. You can try again, or cancel to go back to editing."
    ),
    e!("mesh_dialog.detail_label_x", "細かさ", "Detail"),
    e!(
        "mesh_dialog.detail_hover",
        "大きいほど滑らかになります。容量の上限で失敗したときは小さくして再試行できます",
        "Larger is smoother. If it fails at the size limit, lower it and try again"
    ),
    e!("mesh_dialog.try_again", "もう一度", "Try again"),
    e!("mesh_dialog.cancel", "取消", "Cancel"),
    e!("mesh_dialog.calculating", "立体の表面までの距離を計算しています", "Calculating distance to the solid's surface"),
    e!("mesh_dialog.size", "大きさ: {0}", "Size: {0}"),
    e!("mesh_dialog.unit_sec", "秒", "sec"),
    e!(
        "sketch_live.created_hint",
        "{0} を作成しました。輪郭を変えるには「スケッチを編集」を使ってください",
        "Created {0} (use Edit Sketch to change the outline)"
    ),
    e!(
        "sketch_live.adjust_hint",
        "プレビューを調整  ·  右クリック / Enter: 確定  ·  Esc: キャンセル",
        "Adjust the preview  ·  Right-click / Enter: confirm  ·  Esc: cancel"
    ),
    e!("sketch_live.height_hint", "高さ。表示の矢印をドラッグするか、数値を入力してください", "Height. Drag the arrow in the view, or type a number"),
    e!("sketch_live.top_size_hint", "上面の大きさ（1 = 底面と同じ大きさ）", "Top size (1 = as big as the bottom)"),
    e!("sketch_live.tube_radius_hint", "管の半径", "Radius of the tube"),
    e!("sketch_live.profile_scale", "断面の倍率", "Profile scale"),
    e!("sketch_live.top_outline", "上面の輪郭", "Top outline"),
    e!("sketch_live.top_outline_hint", "上面の輪郭を自分で描く", "Draw the top outline yourself"),
    e!("sketch_live.both_sides", "両側", "Both sides"),
    e!("sketch_live.both_sides_hint", "面の両側に同じ量だけ広げます", "Grow equally to both sides of the plane"),
    e!("sketch_live.holes_button", "穴", "Holes"),
    e!("sketch_live.holes_hint", "このソリッドに穴をあけます", "Cut holes into this solid"),
    e!("sketch_live.confirm_button", "確定", "Confirm"),
    e!("sketch_live.confirm_hint", "残す（Enter または右クリック）", "Keep it (Enter or right-click)"),
    e!("sketch_live.cancel_hint", "この操作をキャンセル（Esc）", "Cancel this operation (Esc)"),
    e!(
        "import_glb.image_texture_note",
        "GLB の画像テクスチャ（ベースカラー、法線など）は読み込めません。代わりに単色を使います",
        "GLB image textures (base colour, normals, etc.) can't be loaded. A plain colour is used instead"
    ),
    e!("import_glb.extended_material_note", "GLB の拡張マテリアルは完全には再現できません", "GLB extended materials can't be fully reproduced"),
    e!(
        "import_glb.transparency_note",
        "GLB の透明度はガラスとして近似します（アルファブレンドとは同じではありません）",
        "GLB transparency is approximated as glass (not the same as alpha blending)"
    ),
    e!("import_glb.glow_note", "GLB の発光色は再現されません", "GLB glow colours are not reproduced"),
    e!(
        "import_glb.vertex_colour_note",
        "GLB の頂点カラー（COLOR_0）は単色の SDF に変換できません",
        "GLB vertex colours (COLOR_0) can't be turned into a single-colour SDF"
    ),
    e!(
        "import_glb.vertex_only_note",
        "GLB は頂点カラーのみのため、標準のマテリアルを使います",
        "GLB has only vertex colours, so the default material is used"
    ),
    e!("import_glb.sparse_unsupported", "スパース形式の GLB データには対応していません", "Sparse GLB data is not supported"),
    e!("import_glb.external_data_unsupported", "GLB の外に保存されたデータには対応していません", "Data stored outside the GLB is not supported"),
    e!("import_glb.compressed_unsupported", "圧縮された GLB（Draco など）には対応していません", "Compressed GLB (Draco etc.) is not supported"),
    e!("import_glb.nodes_too_deep", "GLB の要素の入れ子が深すぎます", "The GLB nodes are nested too deeply"),
    e!("import_glb.too_many_nodes", "GLB の要素が多すぎます", "Too many nodes in the GLB"),
    e!("import_glb.version_unsupported", "対応しているのは glTF 2.0 の GLB だけです", "Only glTF 2.0 GLB is supported"),
    e!("import_glb.too_large", "GLB が大きすぎます", "The GLB is too large"),
    e!("sketch_saved.max_points", "スケッチの点は {0} 個までです", "A sketch can have up to {0} points"),
    e!("sketch_saved.open_path_no_holes", "開いたスケッチのパスには穴をあけられません", "An open sketch path cannot have holes"),
    e!("sketch_saved.invalid_plane", "有効なスケッチ面を選んでください", "Choose a valid sketch plane"),
    e!("sketch_saved.coords_finite", "スケッチの座標には有効な数値を使ってください", "Sketch coordinates must be finite"),
    e!("sketch_saved.name_sketch", "スケッチ", "Sketch"),
    e!("sketch_saved.name_sketch_path", "スケッチのパス", "Sketch path"),
    e!("sketch_saved.choose_saved", "保存済みのスケッチを選んでください", "Choose a saved sketch"),
    e!("sketch_saved.no_plane", "保存済みのスケッチに有効な面がありません", "The saved sketch has no valid plane"),
    e!("sketch_saved.transform_plane", "スケッチの変換ではこの面を表せません", "The sketch transform cannot represent this plane"),
    e!(
        "sketch_saved.coord_limit",
        "編集後のスケッチが変換の座標の上限を超えています",
        "Edited sketch exceeds the coordinate limits of its transform"
    ),
    e!("sketch_saved.choose_open_path", "開いたスケッチのパスを選んでください", "Choose an open sketch path"),
    e!("sketch_saved.sweep_max_points", "スイープのパスの点は {0} 個までです", "A sweep path can have up to {0} points"),
    e!("sketch_saved.top_must_closed", "上のスケッチは、閉じた平面上の輪郭である必要があります", "The top sketch must be a closed, planar outline"),
    e!("sketch_saved.loft_no_holes", "ロフトには穴のない輪郭が必要です", "Loft needs an outline without holes"),
    e!("sketch_saved.top_parallel", "平行な面にある上側のスケッチを選んでください", "Choose a top sketch on a parallel plane"),
    e!(
        "sketch_saved.second_distance",
        "2 つ目のスケッチは最初の面から 0.1–2000 mm の位置に置いてください",
        "Place the second sketch 0.1–2000 mm from the first plane"
    ),
    e!("export_formats.stl_ascii", "STL（テキスト）", "STL (text)"),
    e!(
        "sketch_keep.sketch_gone_msg",
        "このスケッチはもうありません。キャンセルして別のスケッチを選んでください",
        "This sketch no longer exists. Cancel and select another sketch"
    ),
    e!("sketch_keep.max_shapes_msg", "形は最大 {0} 個までです", "Up to {0} shapes"),
    e!("sketch_keep.check_dims_msg", "完了する前に寸法を確認してください", "Check the dimensions before finishing"),
    e!(
        "sketch_keep.saved_next_msg",
        "スケッチを保存しました。それを選んで、Extrude、Revolve、Loft、Sweep のどれかを選んでください",
        "Sketch saved. Select it, then choose Extrude, Revolve, Loft or Sweep"
    ),
    e!(
        "sketch_keep.finish_first_msg",
        "先に Finish Sketch でスケッチを終えてください（右クリック）。その後、ソリッドの操作を選んでください",
        "Finish Sketch first (right-click), then choose a solid operation"
    ),
    e!("sketch_keep.choose_closed_msg", "1つの面の上にある、閉じたスケッチを選んでください", "Choose a closed sketch on one plane"),
    e!("sketch_keep.choose_visible_msg", "表示されている、完成したスケッチを選んでください", "Choose a visible, finished sketch"),
    e!("sketch_keep.choose_different_msg", "別のスケッチを選んでください", "Choose a different sketch"),
    e!("sketch_keep.loft_top_outline_label", "上側の輪郭", "Top outline"),
    e!("sketch_keep.loft_scaled_pill", "拡大・縮小したコピー", "Scaled copy"),
    e!("sketch_keep.loft_saved_pill", "保存したスケッチ", "Saved sketch"),
    e!("sketch_keep.choose_top_sketch", "上側のスケッチを選んでください", "Choose a top sketch"),
    e!("sketch_keep.choose_saved_path", "保存したパスを選んでください", "Choose a saved path"),
    e!(
        "sketch_keep.loft_need_parallel_msg",
        "平行な面にもう1つスケッチを保存してから、Loft を選んでください。",
        "Save a second sketch on a parallel plane, then choose Loft."
    ),
    e!(
        "sketch_keep.sweep_need_open_msg",
        "先に Line で開いたスケッチを保存してから、Sweep を選んでください。",
        "Save an open sketch with Line first, then choose Sweep."
    ),
    e!("import_3mf.too_many_shapes", "3MFの形が多すぎます", "Too many shapes in the 3MF"),
    e!("import_3mf.too_many_parts", "3MFの部品が多すぎます", "Too many parts in the 3MF"),
    e!("import_3mf.too_many_placements", "3MFの配置が多すぎます", "Too many placements in the 3MF"),
    e!("import_3mf.too_many_model_files", "3MFのモデルファイルが多すぎます", "Too many model files in the 3MF"),
    e!("import_3mf.no_model_file", "3MFにモデルファイル {0} がありません", "The 3MF has no model file {0}"),
    e!("import_3mf.parts_nested_deep", "3MFの部品の入れ子が深すぎます", "The 3MF parts are nested too deeply"),
    e!("import_3mf.no_shape_number", "3MFに番号 {0} の形がありません", "The 3MF has no shape with number {0}"),
    e!("import_3mf.no_model_found", "3MF内にモデル（3D/3dmodel.model）が見つかりません", "No model (3D/3dmodel.model) found in the 3MF"),
    e!(
        "import_3mf.colour_material_unsupported",
        "3MFの色・素材の設定に対応していないか不正です。一部の見た目は保持されません",
        "The 3MF has unsupported or invalid colour/material settings. Some looks will not be kept"
    ),
    e!("import_3mf.image_texture_failed", "3MF内の画像テクスチャを読み込めません", "Image textures in the 3MF can't be loaded"),
    e!("recovery.file_too_big", "ファイルが安全上限の 128 MiB を超えています", "The file is over the 128 MiB safety limit"),
    e!("recovery.damaged_read_stopped", "壊れた設計データの読み込みを中止しました", "Reading the damaged design data was stopped"),
    e!("recovery.no_file_name", "保存先にファイル名がありません", "The save location has no file name"),
    e!("recovery.temp_file_failed", "一時ファイルを作れませんでした", "Could not create a temp file"),
    e!("recovery.unsupported_format", "未対応の復旧データ形式です: {0}", "Unsupported recovery data format: {0}"),
    e!(
        "recovery.checksum_mismatch",
        "復旧データのチェックサムが一致しません（壊れている可能性があります）",
        "Recovery data checksum does not match (may be damaged)"
    ),
    e!("recovery.stopped", "中止しました", "Stopped"),
    e!("recovery.file_not_found", "復旧用ファイルが見つかりません", "Recovery file not found"),
    e!("recovery.checkpoint_unexpected_stop", "復旧用の保存処理が予期せず止まりました", "The recovery checkpoint stopped unexpectedly"),
    e!("recovery.checkpoint_disappeared", "復旧用の保存処理が見つからなくなりました", "The recovery checkpoint disappeared"),
    e!("recovery.checkpoint_partway_stop", "復旧用の保存処理が途中で止まりました", "The recovery checkpoint stopped partway through"),
    e!("recovery.checkpoint_start_failed", "復旧用の保存処理を開始できませんでした: {0}", "Could not start the recovery checkpoint: {0}"),
    e!("import_inflate.too_many_codes", "符号が多すぎます", "Too many codes"),
    e!("import_inflate.no_end_code", "終了符号がありません", "No end code"),
    e!("import_inflate.zip64_unsupported", "大きな ZIP64 ファイルには対応していません", "Large ZIP64 files are not supported"),
    e!("import_inflate.too_many_zip_files", "ZIP内のファイルが多すぎます", "Too many files in the ZIP"),
    e!("import_inflate.password_unsupported", "パスワード付きの ZIP には対応していません", "Password-protected ZIPs are not supported"),
    e!("import_inflate.method_unsupported", "この ZIP の圧縮方式には対応していません", "This ZIP compression method is not supported"),
    e!("svg.too_big", "ファイルが大きすぎます（2 MBまで）", "File is too big (up to 2 MB)"),
    e!("svg.not_svg", "SVGファイルではないようです", "This doesn't look like an SVG file"),
    e!(
        "svg.missing_reference",
        "参照先のSVG形状がないか、自分自身を指しています。SVGエディタで参照を展開して、形にしてください",
        "A referenced SVG shape is missing or refers to itself. Expand the references into shapes in your SVG editor"
    ),
    e!(
        "svg.no_filled_line_only",
        "塗りつぶされた形が見つかりません（線だけの図は読み込めません）",
        "No filled shape found (line-only drawings can't be read)"
    ),
    e!("svg.too_small", "形が小さすぎて読み込めません", "Shape is too small to read"),
    e!(
        "svg.complexity_limit",
        "SVGが複雑すぎるため、一部の細かい部分は読み込まれませんでした",
        "SVG complexity limit reached; some details were not loaded"
    ),
    e!(
        "svg.skipped_references",
        "参照先がないか自分自身を指すSVGの参照を飛ばしました",
        "Skipped SVG references whose target is missing or refers to itself"
    ),
    e!("svg.unsupported_element", "未対応のSVG要素または設定: {0}（読み込んでいません）", "Unsupported SVG element or setting: {0} (not loaded)"),
    e!("atomic_files.restore_failed", "{0} を元に戻せませんでした: {1}（バックアップ: {2}）", "Could not restore {0}: {1} (backup: {2})"),
    e!("atomic_files.not_a_file", "保存先がファイルではありません: {0}", "The destination is not a file: {0}"),
    e!("atomic_files.protect_failed", "古いファイルを保護できませんでした: {0}", "Could not protect the old file: {0}"),
    e!("atomic_files.check_failed", "保存先を確認できませんでした: {0}", "Could not check the destination: {0}"),
    e!("atomic_files.finish_failed", "出力ファイルを完成できませんでした: {0}", "Could not finish the output file: {0}"),
    e!("atomic_files.no_files", "書き出すファイルがありません", "There are no files to write"),
    e!("atomic_files.shared_destination", "2つの出力が同じ保存先を指しています: {0}", "Two outputs share a destination: {0}"),
    e!("atomic_files.work_file_failed", "作業用ファイル {0} を作れませんでした: {1}", "Could not create a work file {0}: {1}"),
    e!("atomic_files.write_failed", "書き込みに失敗しました: {0}", "Write failed: {0}"),
    e!("atomic_files.commit_prepare_failed", "ファイルを確定できる状態にできませんでした: {0}", "Could not prepare the file for commit: {0}"),
    e!("document_job.stopped_partway", "ファイルの処理が途中で止まりました", "The file job stopped partway through"),
    e!("document_job.start_failed", "ファイルの処理を開始できませんでした: {0}", "Could not start the file job: {0}"),
    e!("document_job.too_large", "設計データが大きすぎます（最大 {0} MiB）", "The design is too large (max {0} MiB)"),
    e!("document_job.stopped_unexpected", "ファイルの処理が予期せず止まりました", "The file job stopped unexpectedly"),
    e!(
        "document_job.finishing_no_stop",
        "ファイルを仕上げ中です（元のファイルを守るため、止めることはできません）",
        "Finishing the file (cannot be stopped, to protect the original)"
    ),
    e!("document_job.waiting_stop", "中止の完了を待っています…", "Waiting for the stop to finish…"),
    e!("document_job.reading_file", "ファイルを読み込み中", "Reading the file"),
    e!("document_job.checking_shape", "形のデータを確認中", "Checking the shape data"),
    e!("document_job.preparing_save", "保存データを準備中", "Preparing the save data"),
    e!("document_job.preparing_finished", "完成したファイルを準備中", "Preparing the finished file"),
    e!("backend.vulkan", "Vulkan", "Vulkan"),
    e!("backend.dx12", "DirectX 12", "DirectX 12"),
    e!("backend.gl", "OpenGL", "OpenGL"),
    e!("backend.metal", "Metal", "Metal"),
    e!(
        "backend.env_invalid",
        "MELTCAD_BACKEND={0} は使えない値です（vulkan / dx12 / gl / metal / auto のどれかにしてください）",
        "MELTCAD_BACKEND={0} is not a known value (vulkan / dx12 / gl / metal / auto)"
    ),
    e!("backend.exe_not_found", "プログラムの場所を見つけられません: {0}", "Cannot find the program: {0}"),
    e!("backend.restart_failed", "再起動できませんでした: {0}", "Could not restart: {0}"),
    e!(
        "export_job.writing_no_stop",
        "ファイルを書き出し中（この最後の工程は止められません）…",
        "Writing the file (this last step cannot be stopped)…"
    ),
    e!("export_job.building", "作成中…", "Building…"),
    e!("render_job.unexpected_stop", "画像の処理が予期せず止まりました", "The picture job stopped unexpectedly"),
    e!("render_job.stopped_partway", "画像の処理が途中で止まりました", "The picture job stopped partway through"),
    e!("render_job.start_failed", "画像の処理を開始できませんでした: {0}", "Could not start the picture job: {0}"),
    e!("render_job.empty", "画像が空になりました", "The picture came out empty"),
    e!("render_job.no_file_name", "出力先にファイル名がありません", "The output location has no file name"),
    e!("render_job.cannot_write", "{0} に書き込めません: {1}", "Cannot write {0}: {1}"),
    e!("render_job.no_work_file", "{0} の作業用ファイルを作れません", "Cannot create a work file for {0}"),
    e!("render_job.cannot_finish", "{0} を完成できません: {1}", "Cannot finish {0}: {1}"),
    e!("mesh_import.up_auto", "自動", "Auto"),
    e!("mesh_import.up_y", "Y up", "Y up"),
    e!("mesh_import.up_z", "Z up", "Z up"),
    e!(
        "mesh_import.up_auto_desc",
        "ファイルの種類で決めます（OBJ と GLB は Y up、それ以外は Z up）",
        "Picks by file type (OBJ and GLB are Y up, others Z up)"
    ),
    e!("mesh_import.up_y_desc", "Y up のデータです。読み込み時に立てます", "Y-up data. Stands it up on import"),
    e!("mesh_import.up_z_desc", "Z up のデータです。そのまま読み込みます", "Z-up data. Imported as is"),
    e!("mesh_import.default_name", "メッシュ", "Mesh"),
    e!("mesh_import.invalid_dims", "寸法が不正です", "Invalid dimensions"),
    e!(
        "mesh_import.err_out_of_range",
        "立体が Melt の対応範囲を超えています（各辺 0.01〜2000 mm、中心位置 ±5000 mm）。SolidCraft で寸法・位置を調整してください",
        "The solid is outside Melt's supported range (each edge 0.01–2000 mm, center within ±5000 mm). Adjust its size and position in SolidCraft"
    ),
    e!("gpu.step_read_shader", "シェーダーを読み込み中 (create_shader_module)", "Read shader (create_shader_module)"),
    e!("gpu.shader_error", "シェーダーのエラー:\n{0}", "Shader error:\n{0}"),
    e!("gpu.step_render_pipeline", "シーン描画パイプライン (create_render_pipeline)", "Scene render pipeline (create_render_pipeline)"),
    e!("gpu.panic", "予期しない異常が起きました: {0}", "Unexpected error: {0}"),
    e!("gpu.step_prepare_frame", "描画フレームの準備 (prepare)", "Prepare frame (prepare)"),
    e!("gpu.no_adapter", "使えるグラフィック装置が見つかりません: {0}", "No graphics adapter found: {0}"),
    e!("gpu.no_device", "グラフィック装置を作れませんでした: {0}", "Could not create a graphics device: {0}"),
    e!("sdf_volume.err_too_complex", "メッシュが複雑すぎて、内側と外側を判定できません", "Mesh is too complex to check inside and outside"),
    e!("sdf_volume.err_internal", "内部エラーが発生しました", "Internal error"),
    e!("sdf_volume.err_grid", "グリッドを作れません", "Can't make the grid"),
    e!(
        "sketch_edit_ops.err_invalid_outline",
        "元の輪郭が正しくありません。交差している部分や短すぎる線分を先に直してください",
        "Original outline is invalid. Fix crossings or tiny segments first"
    ),
    e!(
        "sketch_edit_ops.err_invalid_value",
        "距離と半径は 0 より大きい数値にしてください",
        "Distance and radius must be finite values greater than 0"
    ),
    e!("sketch_edit_ops.err_too_few_edges", "これ以上、辺を削除できません", "Cannot remove more edges"),
    e!("sketch_edit_ops.err_parallel", "ほぼ平行な線は、安全に交わる点を作れません", "Nearly parallel lines have no safe intersection"),
    e!(
        "sketch_edit_ops.err_cannot_trim",
        "この辺は切り取れません。両隣の辺が安全につながりません",
        "This edge can't be trimmed; neighbors don't connect safely"
    ),
    e!("sketch_edit_ops.err_too_large", "距離または半径が大きすぎるか、角が短すぎます", "Distance or radius too large, or corners too short"),
    e!(
        "sketch_edit_ops.err_too_many_points",
        "曲線の精度を保つと、点の数の上限を超えてしまいます",
        "Keeping curve accuracy would exceed the point limit"
    ),
    e!(
        "sketch_edit_ops.err_invalid_result",
        "適用できません。線の交差、輪郭の裏返し、極端に短い線分のどれかが起きてしまいます",
        "Can't apply: would create self-intersections, flipped outline, or tiny segments"
    ),
    e!("library.lamp_style_name", "デザインランプ（{0}）", "Designer Lamp（{0}）"),
    e!("library.tag_chess", "チェス", "Chess"),
    e!("library.tag_chess_set", "チェスセット", "Chess Set"),
    e!("library.cat_people", "ひと", "People"),
    e!("library.cat_hands", "て", "Hands"),
    e!("library.cat_animals", "どうぶつ", "Animals"),
    e!("library.cat_tools", "どうぐ", "Tools"),
    e!("library.cat_household", "くらしのもの", "Household items"),
    e!("library.cat_furniture", "かぐ", "Furniture"),
    e!("library.cat_food", "たべもの", "Food"),
    e!("library.cat_vehicles", "のりもの", "Vehicles"),
    e!("library.cat_buildings", "たてもの", "Buildings"),
    e!("library.cat_nature", "しぜん", "Nature"),
    e!("library.cat_3d_print", "3Dプリント", "3D printing"),
    e!("library.cat_print_in_place", "プリントインプレース", "Print-in-place"),
    e!("library.cat_realistic_people", "リアルなひと", "Realistic people"),
    e!("library.cat_stylized", "デフォルメ", "Stylized"),
    e!("library.cat_town", "まち", "Town"),
    e!("library.extra_chess", "チェス", "Chess"),
    e!("library.tag_organic_vase", "有機 花瓶", "Organic vase"),
    e!("library.tag_lighting", "照明", "Lighting"),
    e!("library.model_parse_failed", "モデルデータの解析に失敗しました", "Failed to parse model data"),
    e!("organic_vase.name_flow", "オーガニック花瓶 — フロー", "Organic Vase — Flow"),
    e!("organic_vase.name_ripple", "オーガニック花瓶 — 波紋", "Organic Vase — Ripple"),
    e!("organic_vase.name_pebble", "オーガニック花瓶 — 小石", "Organic Vase — Pebble"),
    e!("organic_vase.design_flow", "フロー", "Flow"),
    e!("organic_vase.design_ripple", "波紋", "Ripple"),
    e!("organic_vase.design_pebble", "小石", "Pebble"),
    e!("organic_vase.heading", "Organic Vase｜有機的な花瓶", "Organic Vase"),
    e!("organic_vase.shape_samples", "形のサンプル", "Shape samples"),
    e!("organic_vase.max_diameter", "最大直径", "Max diameter"),
    e!("organic_vase.mouth_diameter", "口径", "Mouth diameter"),
    e!("organic_vase.base_width", "底の幅", "Base width"),
    e!("organic_vase.wave_height", "波の高さ", "Wave height"),
    e!("organic_vase.side_wave", "側面のうねり", "Side waviness"),
    e!("organic_vase.waist", "くびれ", "Waist"),
    e!("organic_vase.roundness", "丸み", "Roundness"),
    e!("organic_vase.waves_around", "円周の波の数", "Number of waves around"),
    e!(
        "organic_vase.min_wall_warning",
        "⚠ 推定最小肉厚 {0} mm（底から約 {1} mm、0.8 mm未満）",
        "⚠ Estimated minimum wall {0} mm (about {1} mm from the base, under 0.8 mm)"
    ),
    e!(
        "organic_vase.wall_warning_tip",
        "壁厚を増やすか、波・くびれを弱めてください。スライサーでも確認してください。",
        "Increase the wall thickness or reduce the waves and waist. Also check in your slicer"
    ),
    e!("organic_vase.min_wall_info", "推定最小肉厚 {0} mm（底から約 {1} mm）", "Estimated minimum wall {0} mm (about {1} mm from the base)"),
    e!(
        "organic_vase.hollow_note",
        "底は閉じて中は空洞です。実際の印刷可否はノズル径・スライサー設定で確認してください。",
        "The base is closed and the inside is hollow. Check whether it prints in practice using nozzle size and slicer settings"
    ),
    e!("designer_chair.style_shell", "デザインチェア / シェル", "Designer Chair / Shell"),
    e!("designer_chair.style_four_leg", "デザインチェア / 四本脚", "Designer Chair / Four-Leg"),
    e!("designer_chair.style_monocoque", "デザインチェア / 一体型", "Designer Chair / Monocoque"),
    e!("designer_chair.style_shell_x", "シェル型", "Shell"),
    e!("designer_chair.style_four_leg_x", "4本脚型", "Four-leg"),
    e!("designer_chair.style_monocoque_x", "一体成形型", "Monocoque"),
    e!("designer_chair.part_seat", "座面", "Seat"),
    e!("designer_chair.part_back", "背もたれ", "Backrest"),
    e!("designer_chair.part_shell_left_arm", "シェル左アーム", "Shell left arm"),
    e!("designer_chair.part_shell_right_arm", "シェル右アーム", "Shell right arm"),
    e!("designer_chair.part_underseat_frame", "座面下フレーム", "Under-seat frame"),
    e!("designer_chair.part_leg_n", "脚 {0}", "Leg {0}"),
    e!("designer_chair.part_base", "台座", "Base"),
    e!("designer_chair.slider_seat_height", "座面の高さ mm", "Seat height mm"),
    e!("designer_chair.slider_seat_width", "座面の幅 mm", "Seat width mm"),
    e!("designer_chair.slider_seat_depth", "座面の奥行き mm", "Seat depth mm"),
    e!("designer_chair.slider_back_angle", "背もたれ角度 °", "Backrest angle °"),
    e!("designer_chair.slider_leg_count", "脚の本数", "Number of legs"),
    e!("designer_chair.slider_leg_radius", "脚の太さ (半径) mm", "Leg thickness (radius) mm"),
    e!("designer_chair.slider_roundness", "全体の丸み mm", "Overall roundness mm"),
    e!(
        "designer_chair.help_text",
        "座面・背もたれ・脚は左のシーン一覧から個別編集できます。全体のスライダーを動かすと自動生成パーツの形状を再計算します（手動で追加したパーツは残ります）。",
        "You can edit the seat, backrest and legs one by one from the scene list on the left. Moving the overall sliders recalculates the auto-generated parts (manually added parts are kept)."
    ),
    e!(
        "designer_chair.max_parts_reached",
        "これ以上パーツを増やせません（シーンは{0}個まで）。不要な形を削除してください。",
        "Can't add more parts (up to {0} per scene). Delete unneeded shapes."
    ),
    e!("designer_chair.tag_chair", "椅子", "Chair"),
    e!("designer_chair.tag_chair_kana", "いす", "Chair"),
    e!("designer_chair.added_decoration", "追加した飾り", "Added decoration"),
    e!("gpu_status.draw_error", "描画中にエラーが {0} 回発生しました: {1}", "Error while drawing, {0} times: {1}"),
    e!(
        "gpu_status.preparing",
        "グラフィックを準備中です… 初回は時間がかかることがあります（経過時間 {0}）",
        "Preparing graphics… the first run can take a while (elapsed {0})"
    ),
    e!("gpu_status.err_oom", "メモリ不足 (OutOfMemory)", "Out of memory (OutOfMemory)"),
    e!("gpu_status.err_validation", "検証エラー (Validation)", "Validation error (Validation)"),
    e!("gpu_status.err_internal", "内部エラー (Internal)", "Internal error (Internal)"),
    e!("gif.err_delay", "すべてのコマに表示時間を設定してください", "Every frame needs a delay"),
    e!("gif.err_size", "画像のサイズが正しくありません", "Invalid image size"),
    e!("gif.err_frame_size", "コマのサイズが正しくありません", "A frame has the wrong size"),
    e!("organic_tube.preset_wave", "オーガニックチューブ · ウェーブ", "Organic Tube · Wave"),
    e!("organic_tube.preset_ring", "オーガニックチューブ · リング", "Organic Tube · Ring"),
    e!("organic_tube.preset_s", "オーガニックチューブ · S", "Organic Tube · S"),
    e!("organic_tube.group_name", "オーガニックチューブ · {0}", "Organic Tube · {0}"),
    e!("organic_tube.preset_wave_x", "波型", "Wave"),
    e!("organic_tube.preset_ring_x", "リング型", "Ring"),
    e!("organic_tube.preset_s_x", "S字型", "S-shape"),
    e!("organic_tube.limit", "形またはグループの上限です。不要な形を減らしてください", "Shape or group limit reached. Remove some unneeded shapes."),
    e!("organic_tube.added", "Organic Tube {0}を追加。点をドラッグ／太さを調整できます", "Added Organic Tube {0}. Drag points or adjust thickness."),
    e!("chess.group_name", "チェス/{0}/{1}/{2} ({3})", "Chess/{0}/{1}/{2} ({3})"),
    e!("chess.piece_king", "キング", "King"),
    e!("chess.piece_queen", "クイーン", "Queen"),
    e!("chess.piece_rook", "ルーク", "Rook"),
    e!("chess.piece_bishop", "ビショップ", "Bishop"),
    e!("chess.piece_knight", "ナイト", "Knight"),
    e!("chess.piece_pawn", "ポーン", "Pawn"),
    e!("chess.design_classic", "クラシック", "Classic"),
    e!("chess.design_minimal", "ミニマル", "Minimal"),
    e!("chess.design_organic", "有機的", "Organic"),
    e!("chess.side_white", "白", "White"),
    e!("chess.side_black", "黒", "Black"),
    e!("chess.cross_upright", "十字・たて", "Cross upright"),
    e!("chess.cross_sideways", "十字・よこ", "Cross sideways"),
    e!("chess.crown_base", "王冠の台座", "Crown base"),
    e!("chess.queen_crown", "女王の冠", "Queen's crown"),
    e!("chess.crown_center_post", "王冠の中央の支柱", "Crown center post"),
    e!("chess.crown_ornament", "冠の飾り", "Crown ornament"),
    e!("chess.queen_orb", "女王の宝珠", "Queen's orb"),
    e!("chess.battlement_rim", "城壁のへり", "Battlement rim"),
    e!("chess.organic_battlement", "有機的な城壁", "Organic battlement"),
    e!("chess.battlement_tooth", "城壁の凸", "Battlement tooth"),
    e!("chess.bishop_hat", "司教帽", "Bishop's hat"),
    e!("chess.bishop_hat_groove", "司教帽の斜めの溝", "Bishop's hat diagonal groove"),
    e!("chess.hat_tip", "帽子の先", "Hat tip"),
    e!("chess.horse_head_3d", "立体的な馬の頭", "3D horse head"),
    e!("chess.horse_cheek", "馬の頬", "Horse cheek"),
    e!("chess.horse_eye", "馬の目", "Horse eye"),
    e!("chess.horse_muzzle", "鼻先", "Muzzle"),
    e!("chess.horse_mane", "たてがみ", "Mane"),
    e!("chess.pawn_ball", "ポーンの球", "Pawn ball"),
    e!("chess.board_group", "Chess/盤面 (8×8)", "Chess board (8×8)"),
    e!("chess.board_base", "盤面・土台", "Board base"),
    e!("chess.dark_square", "濃いマス", "Dark squares"),
    e!(
        "chess.set_space_needed",
        "チェスセットを置くには、{0}グループ・{1}形の空きが必要です",
        "Placing a chess set needs {0} free groups and {1} free shapes"
    ),
    e!("chess.single", "単体", "Single"),
    e!("chess.limit_groups_or_shapes", "グループ数または形の数が上限です", "Group or shape count is at the limit"),
    e!("chess.piece_not_found", "駒が見つかりません", "Piece not found"),
    e!("chess.select_piece", "チェスの駒を選択してください", "Select a chess piece"),
    e!("chess.piece_body_not_found", "駒の本体が見つかりません", "Piece body not found"),
    e!("chess.limit_shapes", "形の数が上限です", "Shape count is at the limit"),
    e!("fusion.smooth_fusion", "Smooth Fusion", "Smooth Fusion"),
    e!("fusion.smooth_union", "Smooth Union", "Smooth Union"),
    e!("fusion.preset_triple_blob", "Smooth Fusion · 3連ブロブ", "Smooth Fusion · Triple blob"),
    e!("fusion.preset_four_leaf", "Smooth Fusion · 四つ葉", "Smooth Fusion · Four-leaf"),
    e!("fusion.preset_wave", "Smooth Fusion · ウェーブ", "Smooth Fusion · Wave"),
    e!("fusion.preset_branch", "Smooth Fusion · 分岐", "Smooth Fusion · Branch"),
    e!("fusion.preset_mix", "Smooth Fusion · ミックス", "Smooth Fusion · Mix"),
    e!("fusion.preset_cloud", "Smooth Fusion · クラウド", "Smooth Fusion · Cloud"),
    e!("fusion.name_smooth_fusion", "スムーズフュージョン", "Smooth fusion"),
    e!("fusion.name_smooth_merge", "なめらか合体", "Smooth merge"),
    e!("fusion.name_puffy", "ぷっくり", "Puffy"),
    e!("fusion.count", "{0}個", "{0} pcs"),
    e!(
        "perf.tip_overlay",
        "1秒あたりの描画回数、CPUとグラフィックの処理時間、画面の描画スケールを表示します",
        "Shows frames per second, CPU and GPU time, and the draw scale over the view"
    ),
    e!(
        "perf.tip_speed_test",
        "いくつかのシーンを描いて、このパソコンの速さを表で表示します",
        "Draws a few scenes and shows this machine's speed in a table"
    ),
    e!("png.err_zero_size", "PNG のサイズは 0 より大きくしてください（指定値: {0}x{1}）", "PNG size must be non-zero, got {0}x{1}"),
    e!("png.err_max_side", "PNG の辺は最大 {0} ピクセルまでです（指定値: {1}x{2}）", "PNG sides must be at most {0} pixels, got {1}x{2}"),
    e!("png.err_chunk_too_large", "PNG のデータが大きすぎます", "PNG chunk too large"),
    e!("stroke.plane_floor", "床", "Floor"),
    e!("stroke.plane_front", "正面", "Front"),
    e!("stroke.plane_side", "側面", "Side"),
    e!("precise.file_name", "SolidCraft", "SolidCraft"),
    e!(
        "precise.import_in_progress_switch",
        "立体の取り込みが終わるか、取り消してから画面を切り替えてください",
        "Finish importing the solid or cancel it before switching screens"
    ),
    e!(
        "precise.import_finish_switch",
        "取り込みが終わるか、取り消してから画面を切り替えてください",
        "Finish the import or cancel it before switching screens"
    ),
    e!(
        "precise.open_confirm",
        "今の操作を確定するか、取り消してから精密ソリッドを開いてください",
        "Confirm or undo the current action, then open the precise solid"
    ),
    e!(
        "precise.back_blocked",
        "スケッチや立体の操作を完了するか、取り消してから戻ってください",
        "Finish or undo the sketch or solid operation before going back"
    ),
    e!(
        "precise.error_keep_shape",
        "{0} 精密ソリッドの形は残っています。戻り直すと再び取り込めます",
        "{0} Precise solid shapes are still there. Go back to import them again."
    ),
    e!("precise.import_cancelled", "取り込みを取り消しました", "Import cancelled"),
    e!("precise.back_button", "Melt 3D に戻る", "Back to Melt 3D"),
    e!(
        "precise.back_hover",
        "作った立体を寸法・位置を保って自動で取り込みます。スケッチや操作は先に完了してください",
        "Import the solid automatically, keeping its size and position. Finish any sketch or operation first."
    ),
    e!("precise.autosave_failed", "精密データの自動保存に失敗しました: {0}", "Automatic saving of precise data failed: {0}"),
    e!("precise.previous_importing", "前の立体を取り込み中です", "Previous solid is still being imported"),
    e!("precise.export_failed", "立体を書き出せませんでした: {0}", "Couldn't export the solid: {0}"),
    e!("precise.read_failed", "立体を読み取れませんでした: {0}", "Couldn't read the solid: {0}"),
    e!("precise.importing_title", "SolidCraft の立体を Melt に取り込み中", "Importing SolidCraft solid into Melt"),
    e!(
        "precise.importing_note",
        "形を書き出して読み込んでいます。大きさと位置はそのままです",
        "Exporting and loading the shape. Size and position are unchanged."
    ),
    e!("precise.cancelling_note", "取り消しています。書き出しが終わるまでお待ちください…", "Cancelling. Please wait until the export finishes…"),
    e!("precise.elapsed", "{0} 秒", "{0} sec"),
    e!("precise.abort", "中止", "Stop"),
    e!("precise.no_data", "この形には SolidCraft のデータがありません", "This shape has no SolidCraft data"),
    e!("precise.finish_solidcraft_first", "SolidCraft の作業を先に終えてください", "Finish your SolidCraft work first"),
    e!("precise.cannot_edit", "この形を編集できません", "This shape can't be edited"),
    e!("precise.prepare_data_failed", "データを用意できません: {0}", "Couldn't prepare data: {0}"),
    e!("precise.open_failed", "SolidCraft を開けませんでした", "Couldn't open SolidCraft"),
    e!("precise.temp_path_unreadable", "一時ファイルの場所を読み取れません", "Can't read the temporary file location"),
    e!("precise.temp_prepare_failed", "一時ファイルを用意できません: {0}", "Couldn't prepare temporary file: {0}"),
    e!("precise.temp_prepare_failed_plain", "一時ファイルを用意できませんでした", "Couldn't prepare the temporary file"),
    e!("sketch_plane_pick.draw_on_face", "この面に描く", "Draw on this face"),
    e!("sketch_plane_pick.draw_on_floor", "床に描く", "Draw on the floor"),
    e!("curve_tool.group_1", "グループ 1", "Group 1"),
    e!(
        "curve_tool.section_round_hint",
        "丸いSection。幅の比率で楕円にもできます",
        "Round section. You can also make it an oval using the width ratio."
    ),
    e!("curve_tool.section_soft_square_hint", "四隅が滑らかな角丸状のSection", "Section with smoothly rounded corners"),
    e!("curve_tool.section_width_label", "断面の幅", "Section width"),
    e!("curve_tool.section_width_hint", "1 = 円、0.45 = 細長い楕円 / 角丸断面", "1 = circle, 0.45 = slender ellipse / rounded-corner section"),
    e!("designer_bag.name", "デザインバッグ", "Designer Bag"),
    e!("designer_bag.name_tote", "デザイナーバッグ・トート", "Designer Bag・Tote"),
    e!("designer_bag.name_half_moon", "デザイナーバッグ・半円", "Designer Bag・Half moon"),
    e!("designer_bag.name_origami", "デザイナーバッグ・折り紙", "Designer Bag・Origami"),
    e!("designer_bag.part_body", "本体（中空）", "Body (hollow)"),
    e!("designer_bag.part_base", "底面", "Base"),
    e!("designer_bag.part_opening", "開口部（くり抜き）", "Opening (cut out)"),
    e!("designer_bag.part_handle_front", "持ち手・前", "Handle (front)"),
    e!("designer_bag.part_handle_back", "持ち手・後", "Handle (back)"),
    e!("designer_bag.size_heading", "Designer Bag | 寸法・形状", "Designer Bag | Size and shape"),
    e!("designer_bag.width_label", "横幅", "Width"),
    e!("designer_bag.width_hint", "バッグ本体の幅（mm）", "Width of the bag body (mm)"),
    e!("designer_bag.height_label", "高さ", "Height"),
    e!("designer_bag.height_hint", "持ち手を除く高さ（mm）", "Height excluding the handle (mm)"),
    e!("designer_bag.depth_label", "奥行き", "Depth"),
    e!("designer_bag.depth_hint", "本体の前後の厚み（mm）", "Front-to-back thickness of the body (mm)"),
    e!("designer_bag.wall_label", "壁厚", "Wall thickness"),
    e!("designer_bag.wall_hint", "中空にした本体の壁の厚さ（mm）", "Wall thickness of the hollowed body (mm)"),
    e!("designer_bag.handle_length_label", "持ち手長さ", "Handle length"),
    e!("designer_bag.handle_length_hint", "バッグ上端からの持ち手の立ち上がり（mm）", "How far the handle rises above the top of the bag (mm)"),
    e!("designer_bag.handle_thickness_label", "持ち手太さ", "Handle thickness"),
    e!("designer_bag.handle_thickness_hint", "持ち手の直径（mm）", "Handle diameter (mm)"),
    e!("designer_bag.handle_curve_label", "持ち手曲率", "Handle curve"),
    e!("designer_bag.handle_curve_hint", "アーチの膨らみ方", "How much the arch bulges"),
    e!(
        "designer_bag.thin_wall_warning",
        "薄い部分があります。印刷前に肉厚を確認してください",
        "Some parts are thin. Check the wall thickness before printing."
    ),
    e!(
        "designer_bag.design_wall_thickness",
        "設計壁厚 {0} mm（実際の最薄部はプリントチェックで確認）",
        "Design wall thickness {0} mm (check the thinnest actual part with Print Check)"
    ),
    e!("designer_bag.print_check_button", "プリントチェック", "Print check"),
    e!(
        "designer_bag.print_check_hint",
        "シーン全体のメッシュ肉厚・穴・張り出しを確認",
        "Check wall thickness, holes and overhangs across the whole scene"
    ),
    e!(
        "designer_bag.edit_parts_help",
        "パーツ別の編集は左のツリーで「本体」「底面」「持ち手」を選択。寸法を変えるとパーツ形状を再生成します。",
        "Edit parts one at a time: select 'Body', 'Base' or 'Handle' in the tree on the left. Changing the size regenerates the part shapes."
    ),
    e!("designer_lamp.group_name", "デザインランプ - {0}", "Designer Lamp - {0}"),
    e!("designer_lamp.type_mushroom", "マッシュルーム型", "Mushroom type"),
    e!("designer_lamp.type_globe", "球体型", "Globe type"),
    e!("designer_lamp.type_arch", "アーチ型", "Arch type"),
    e!("designer_lamp.name_shade", "傘 / Shade", "Shade"),
    e!("designer_lamp.name_arch_arm", "支柱 / Arch arm", "Arch arm"),
    e!("designer_lamp.name_stem", "支柱 / Stem", "Stem"),
    e!("designer_lamp.name_base", "土台 / Base", "Base"),
    e!("designer_lamp.lamp_not_found", "ランプが見つかりません", "Lamp not found"),
    e!("designer_lamp.needs_three_parts", "傘・支柱・土台の3部品が必要です", "Needs 3 parts: shade, stem and base"),
    e!("designer_lamp.unlink_before_split", "リンクやパターンを解除してから分割してください", "Unlink or clear patterns before splitting"),
    e!("designer_lamp.group_limit", "グループ数が上限に達しています", "Group limit reached"),
    e!("designer_lamp.print_group", "Designer Lamp 印刷用 - {0}", "Designer Lamp for printing - {0}"),
    e!("designer_lamp.total_height", "全高", "Total height"),
    e!("designer_lamp.lamp_height_hint", "ランプの高さ（mm）", "Lamp height (mm)"),
    e!("designer_lamp.shade_diameter", "傘の直径", "Shade diameter"),
    e!("designer_lamp.shade_diameter_hint", "傘の外径（mm）", "Shade outer diameter (mm)"),
    e!("designer_lamp.curvature", "曲率", "Curvature"),
    e!("designer_lamp.curvature_hint", "傘とアーチのカーブ", "Curve of the shade and arch"),
    e!("designer_lamp.wall_thickness", "壁厚", "Wall thickness"),
    e!("designer_lamp.wall_hint", "傘の肉厚（mm）。強度保証ではありません", "Shade wall thickness (mm). Not a strength guarantee"),
    e!("designer_lamp.hollow", "傘を中空にする（下面に開口）", "Hollow shade (opening on the underside)"),
    e!("designer_lamp.glow", "発光", "Glow"),
    e!("designer_lamp.glow_hint", "見た目の発光。電気部品は含まれません", "Visual glow only. No electrical parts included"),
    e!("designer_lamp.select_part", "部品を選択", "Select part"),
    e!("designer_lamp.part_shade", "傘", "Shade"),
    e!("designer_lamp.part_stem", "支柱", "Stem"),
    e!("designer_lamp.part_base", "土台", "Base"),
    e!(
        "designer_lamp.part_edit_note",
        "選択した部品は右側の位置・回転・材質・輪郭などを個別編集できます。寸法変更で断面は再生成されます。",
        "Selected parts can be edited on the right, including position, rotation, material and outline. Changing dimensions regenerates the cross-section"
    ),
    e!("designer_lamp.split_button", "3Dプリント用に3部品へ分割", "Split into 3 parts for 3D printing"),
    e!(
        "designer_lamp.split_hint",
        "分割すると造形面に別々に並びます。個別選択してSTL/3MFで書き出せます。",
        "Splitting arranges the parts separately on the build plate. Select each one to export as STL or 3MF"
    ),
    e!(
        "designer_lamp.split_done",
        "傘・支柱・土台を分割しました。選択中の部品を書き出せます",
        "Split into shade, stem and base. You can export the selected part"
    ),
    e!("organic_blob.group_name", "オーガニックブロブ · {0}", "Organic Blob · {0}"),
    e!("organic_blob.core_name", "中心の球", "Center sphere"),
    e!("organic_blob.lobe_name", "ふくらみ {0}", "Bulge {0}"),
    e!("organic_blob.group_limit", "グループは{0}個までです", "Up to {0} groups"),
    e!("organic_blob.shape_limit", "形は{0}個までです", "Up to {0} shapes"),
    e!(
        "organic_blob.added",
        "Organic Blobを追加しました。グループ内の球を自由に動かしたり大きさを変えられます",
        "Added Organic Blob. Move the spheres in the group freely or change their sizes."
    ),
    e!("organic_blob.preview_start_error", "プレビューを開始できません: {0}", "Can't start preview: {0}"),
    e!(
        "organic_blob.preview_error",
        "プレビューを表示できません。形の追加と編集はできます",
        "Can't show preview. You can still add and edit shapes."
    ),
    e!("organic_blob.title", "Organic Blob · ぷっくりブロブ", "Organic Blob · Puffy blob"),
    e!("organic_blob.subtitle", "やわらかい球のふくらみを、滑らかにつなげます", "Smoothly connects the soft bulges of spheres"),
    e!("organic_blob.preview_calculating", "プレビューを計算中…", "Calculating preview…"),
    e!("organic_blob.preview_updating", "小さなプレビューを更新中…", "Updating small preview…"),
    e!("organic_blob.sdf_preview_note", "SDFの簡易陰影プレビュー", "Simple shaded SDF preview"),
    e!("organic_blob.bulge_hint", "大きくするほど、ふくらみが外側へ飛び出します", "The larger this is, the more the bulges stick out"),
    e!("organic_blob.detail", "凹凸・ばらつき", "Bumpiness and variation"),
    e!("organic_blob.detail_hint", "ふくらみの数と大小のばらつきを変えます", "Changes the number and size variation of bulges"),
    e!("organic_blob.roundness", "丸み・なめらかさ", "Roundness and smoothness"),
    e!(
        "organic_blob.roundness_hint",
        "球同士のなめらかな接合幅を変えます（既存のSDFメルト）",
        "Changes how smoothly spheres blend together (existing SDF melt)"
    ),
    e!("organic_blob.aspect", "縦横比（高さ／幅）", "Aspect ratio (height / width)"),
    e!("organic_blob.aspect_hint", "1.0は球形、1より大きいと縦長、小さいと平たい形", "1.0 is a sphere, above 1 is taller, below 1 is flatter"),
    e!("organic_blob.seed", "シード", "Seed"),
    e!("organic_blob.new_shape", "別の形 ↻", "Another shape ↻"),
    e!(
        "organic_blob.editable_count",
        "編集できる球 {0}個で生成（高解像度メッシュ化はしません）",
        "Generates {0} editable spheres (not converted to a high-resolution mesh)"
    ),
    e!(
        "organic_blob.melt_zero_warning",
        "メルト量が0です。滑らかな融合にはメルト量を上げてください",
        "Melt amount is 0. Raise it for smooth blending."
    ),
    e!("organic_blob.add_button", "ブロブを追加", "Add blob"),
    e!("organic_blob.stop", "やめる", "Stop"),
    e!(
        "organic_blob.after_add_note",
        "追加後は通常の形として、移動・変形・複製・削除できます",
        "After adding, you can move, transform, duplicate and delete it as a normal shape"
    ),
    e!("panels.group_name", "グループ {0}", "Group {0}"),
    e!("pipe_tool.name", "パイプ", "Pipe"),
    e!("puffy_text.group_name", "ぷっくり文字 {0} ({1})", "Puffy {0} ({1})"),
    e!("puffy_text.align_left", "左揃え", "Left"),
    e!("puffy_text.align_center", "中央揃え", "Center"),
    e!("puffy_text.align_right", "右揃え", "Right"),
    e!("puffy_text.spaces_only_error", "空白だけの文字列は使えません（最大{0}文字）", "Text can't be only spaces (up to {0} characters)"),
    e!("puffy_text.control_chars_error", "改行や制御文字には対応していません", "Line breaks and control characters aren't supported"),
    e!(
        "puffy_text.font_needed_error",
        "日本語・英語を含むフォント（TTF/OTF/TTC）を選んでください",
        "Choose a font (TTF, OTF or TTC) that includes Japanese or English"
    ),
    e!("puffy_text.font_path_too_long", "フォントのパスが長すぎます", "Font file path is too long"),
    e!(
        "puffy_text.size_limits_error",
        "文字の大きさ・厚さ・ふくらみ・位置を安全な範囲にしてください",
        "Keep text size, thickness, bulge and position within safe limits"
    ),
    e!("puffy_text.font_type_error", "TTF / TTC / OTF のフォントを選んでください", "Choose a TTF, TTC or OTF font"),
    e!(
        "puffy_text.font_not_found",
        "フォントが見つかりません。以前の輪郭はそのまま利用できます",
        "Font not found. The previous outline can still be used."
    ),
    e!("puffy_text.font_too_large", "フォントが大きすぎるか空です（40MBまで）", "Font is too large or empty (up to 40MB)"),
    e!("puffy_text.font_read_failed", "フォントを読み込めませんでした", "Couldn't load the font"),
    e!("puffy_text.font_outline_read_failed", "フォントの文字輪郭を安全に読み込めませんでした", "Couldn't safely load the font's character outlines"),
    e!("puffy_text.no_closed_outline", "この文字には閉じた輪郭がありません", "This character has no closed outline"),
    e!("puffy_text.outline_too_complex", "文字の輪郭が複雑すぎます（上限を超えました）", "Character outline is too complex (over the limit)"),
    e!(
        "puffy_text.dropped_outlines",
        "細かい輪郭を{0}個省略するため、このフォント・文字は安全に立体化できません",
        "{0} fine outlines would be skipped, so this font or character can't be made into a solid safely"
    ),
    e!("puffy_text.solid_failed", "「{0}」の輪郭を立体にできませんでした", "Couldn't turn the outline of '{0}' into a solid"),
    e!("puffy_text.outline_too_large", "「{0}」の輪郭が大きすぎます", "Outline of '{0}' is too large"),
    e!("puffy_text.holes_close", "「{0}」の穴がつぶれます。ふくらみを小さくしてください", "The holes in '{0}' would close. Reduce the bulge."),
    e!(
        "puffy_text.font_load_failed",
        "このフォントを読み込めません（TTCの番号も確認してください）",
        "Can't load this font (check the TTC face number too)"
    ),
    e!(
        "puffy_text.missing_glyph",
        "選択したフォントに「{0}」がありません。日本語対応フォントを選んでください",
        "The selected font has no '{0}'. Choose a font that supports Japanese."
    ),
    e!("puffy_text.text_width_error", "文字列の幅が大きすぎるか、小さすぎます", "Text width is too large or too small"),
    e!("puffy_text.edit_text_not_found", "再編集する文字列が見つかりません", "Couldn't find the text to edit again"),
    e!(
        "puffy_text.linked_group_edit",
        "リンクされた文字グループは、リンクを解除してから再編集してください",
        "Unlink the character group before editing it again"
    ),
    e!("puffy_text.text_id_limit", "文字列IDの上限です", "Text ID limit reached"),
    e!("puffy_text.no_outline_char", "「{0}」に輪郭がありません", "'{0}' has no outline"),
    e!("puffy_text.no_solid_chars", "立体化できる文字がありません", "No characters can be made into a solid"),
    e!(
        "puffy_text.shape_group_limit",
        "形・グループの上限を超えます（形{0}、グループ{1}まで）。文字数を減らしてください",
        "Over the shape and group limit (up to {0} shapes and {1} groups). Reduce the number of characters."
    ),
    e!("prefs.lang_ja", "日本語", "日本語"),
    e!("prefs.lang_zh_hans", "简体中文", "简体中文"),
    e!("guide.try_box_button", "箱を作ってみる", "Try making a box"),
    e!("context_toolbar.move", "移動", "Move"),
    e!("context_toolbar.rotate", "回転", "Rotate"),
    e!("context_toolbar.scale", "拡大", "Scale"),
    e!("context_toolbar.duplicate", "複製", "Duplicate"),
    e!("context_toolbar.delete", "削除", "Delete"),
    e!("context_toolbar.edit_sketch", "輪郭を編集", "Edit outline"),
    e!("context_toolbar.end_sketch_edit", "編集を終了", "Finish editing"),
    e!("context_toolbar.extrude", "押し出し", "Extrude"),
    e!("context_toolbar.extrude_settings", "押し出し設定", "Extrude settings"),
    e!("context_toolbar.align_x", "X整列", "Align X"),
    e!("context_toolbar.align_y", "Y整列", "Align Y"),
    e!("context_toolbar.align_z", "Z整列", "Align Z"),
    e!("context_toolbar.group", "グループ化", "Group"),
    e!("context_toolbar.frame", "選択を見る", "View selection"),
    e!("context_toolbar.rename", "名前を変更", "Rename"),
    e!("context_toolbar.flip", "足す/削る", "Add/subtract"),
    e!("context_toolbar.tip_move", "ギズモで移動 (W)", "Move with gizmo (W)"),
    e!("context_toolbar.tip_rotate", "ギズモで回転 (E)", "Rotate with gizmo (E)"),
    e!("context_toolbar.tip_scale", "ギズモで拡大縮小 (R)", "Scale with gizmo (R)"),
    e!("context_toolbar.tip_duplicate", "8mmずらして複製 (Ctrl+D)", "Duplicate offset 8 mm (Ctrl+D)"),
    e!("context_toolbar.tip_delete", "選択した形を削除 (Delete)", "Delete selected shapes (Delete)"),
    e!("context_toolbar.tip_edit_sketch", "元の輪郭を点で編集", "Edit the original outline by points"),
    e!("context_toolbar.tip_end_sketch_edit", "編集を終了。形は残ります", "Finish editing. Shapes are kept"),
    e!("context_toolbar.tip_extrude", "完成した2D輪郭を現在の高さで押し出す", "Extrude the finished 2D outline to the current height"),
    e!("context_toolbar.tip_extrude_settings", "右の形パネルで押し出しの高さを変更", "Change the extrude height in the shape panel on the right"),
    e!("context_toolbar.tip_align_x", "ほかの形を主選択のX座標へそろえる", "Align other shapes to the X position of the main selection"),
    e!("context_toolbar.tip_align_y", "ほかの形を主選択のY座標へそろえる", "Align other shapes to the Y position of the main selection"),
    e!("context_toolbar.tip_align_z", "ほかの形を主選択のZ座標へそろえる", "Align other shapes to the Z position of the main selection"),
    e!(
        "context_toolbar.tip_group",
        "選んだ形を新しいグループに移す（メルト結果が変わる場合があります）",
        "Move selected shapes to a new group (this may change the melt result)"
    ),
    e!("context_toolbar.tip_frame", "選んだ形を画面の中央に表示 (Shift+F)", "Show selected shapes in the center of the screen (Shift+F)"),
    e!("context_toolbar.tip_rename", "左のシーンで名前を編集", "Edit the name in the scene on the left"),
    e!("context_toolbar.tip_flip", "足す/削るを切り替える (Tab)", "Switch between add and subtract (Tab)"),
    e!("context_toolbar.err_select_multiple", "形をCtrl＋クリックで複数選んでください", "Select multiple shapes with Ctrl+click"),
    e!("context_toolbar.err_selection_missing", "選択した形が見つかりません", "Selected shape not found"),
    e!("context_toolbar.err_mixed_selection", "形とグループを混ぜて選択できません", "Shapes and groups can't be selected together"),
    e!("context_toolbar.err_different_groups", "別のグループの形は一緒に操作できません", "Shapes from different groups can't be used together"),
    e!("context_toolbar.err_select_two_or_more", "形を2個以上選んでください", "Select 2 or more shapes"),
    e!("context_toolbar.err_select_shape", "形を選択してください", "Select a shape"),
    e!("context_toolbar.err_finish_drag", "ドラッグを終えてから操作してください", "Finish dragging before continuing"),
    e!(
        "context_toolbar.err_finish_active_sketch_or_solid",
        "アクティブなスケッチまたはソリッドを先に終了してください",
        "Finish the active sketch or solid first"
    ),
    e!("context_toolbar.err_gizmo_in_sketch", "スケッチ編集中はギズモを使えません", "Gizmo can't be used while editing a sketch"),
    e!("context_toolbar.err_duplicate_same_group", "同じグループ内の形だけ複製できます", "Only shapes in the same group can be duplicated"),
    e!("context_toolbar.err_shape_limit_would_reach", "形の上限に達します", "This would reach the shape limit"),
    e!("context_toolbar.err_shape_limit_reached", "形の上限に達しました", "Shape limit reached"),
    e!("context_toolbar.err_group_limit_reached", "グループ数の上限に達しました", "Group limit reached"),
    e!("context_toolbar.err_outline_editing", "輪郭の編集中です。先に編集を終了してください", "Still editing an outline. Finish editing first"),
    e!("context_toolbar.err_delete_mixed", "グループと形の混在選択は削除できません", "Can't delete a mix of groups and shapes"),
    e!("context_toolbar.err_select_sketch_shape", "スケッチから作った形を選んでください", "Select a shape made from a sketch"),
    e!("context_toolbar.err_not_editing_outline", "輪郭を編集中ではありません", "Not editing an outline"),
    e!("context_toolbar.err_complete_2d_outline", "2D輪郭を完成させてください", "Complete the 2D outline first"),
    e!(
        "context_toolbar.err_group_needed_for_hole_sketch",
        "穴付きスケッチを作るためのグループが足りません",
        "Not enough groups to make a sketch with holes"
    ),
    e!("context_toolbar.err_select_extruded_shape", "押し出した形を選んでください", "Select an extruded shape"),
    e!("context_toolbar.err_select_two_same_group", "同じグループ内の形を2個以上選んでください", "Select 2 or more shapes in the same group"),
    e!(
        "context_toolbar.err_cannot_split_pattern",
        "パターン・リンク・穴付きスケッチのグループは分割できません",
        "Can't split groups with a pattern, link or hole sketch"
    ),
    e!("context_toolbar.err_cannot_split_link_source", "リンクコピーの元グループは分割できません", "Can't split the source group of a linked copy"),
    e!(
        "context_toolbar.err_select_one_multi",
        "複数選択中は主選択だけに適用されるため、1つ選択してください",
        "Multiple selection only applies to the main selection, so select just one"
    ),
    e!("context_toolbar.copy_suffix", " コピー", " Copy"),
    e!(
        "context_toolbar.grouped_message",
        "グループ化しました。メルトや削りの結果が変わる場合はCtrl+Zで戻せます",
        "Grouped. If the melt or cut result changes, you can undo with Ctrl+Z"
    ),
    e!("context_toolbar.label_move", "移動　W", "Move  W"),
    e!("context_toolbar.label_rotate", "回転　E", "Rotate  E"),
    e!("context_toolbar.label_scale", "拡大縮小　R", "Scale  R"),
    e!("context_toolbar.label_duplicate", "複製　Ctrl+D", "Duplicate  Ctrl+D"),
    e!("context_toolbar.label_delete", "削除　Delete", "Delete  Delete"),
    e!("context_toolbar.hint_select_to_edit", "形を選ぶと編集できます", "Select a shape to edit it"),
    e!("melt_preset.base_sphere", "球体", "Sphere"),
    e!("melt_preset.base_box", "箱", "Box"),
    e!("melt_preset.group_name", "Melt（とろける形状）", "Melt (melting shape)"),
    e!("melt_preset.base_body_name", "元の形状", "Original shape"),
    e!("melt_preset.drip_name", "液だれ {0}", "Drip {0}"),
    e!("melt_preset.err_preview_image", "プレビュー画像を作れませんでした", "Couldn't create the preview image"),
    e!("melt_preset.err_preview_thread", "プレビュースレッドを起動できませんでした", "Couldn't start the preview thread"),
    e!("melt_preset.err_group_not_found", "編集対象のグループが見つかりません", "Group to edit not found"),
    e!("melt_preset.err_edit_link_source", "リンクコピーは元のグループを編集してください", "Edit the original group of a linked copy"),
    e!(
        "melt_preset.err_too_many_shapes",
        "形が{0}個を超えるため追加できません。本数を減らしてください",
        "Can't add: this would go over {0} shapes. Reduce the count"
    ),
    e!("melt_preset.err_no_group_editing", "編集中のグループがありません", "No group is being edited"),
    e!("melt_preset.err_max_groups", "グループは{0}個までです", "Up to {0} groups"),
    e!("melt_preset.heading", "Melt — とろける形状", "Melt — melting shape"),
    e!(
        "melt_preset.intro",
        "球・箱から、重力で垂れる水飴のような形を作ります",
        "Make melting, syrup-like shapes that drip from a sphere or box under gravity"
    ),
    e!("melt_preset.section_base", "元の形", "Original shape"),
    e!("melt_preset.sphere_diameter", "球の直径 (mm)", "Sphere diameter (mm)"),
    e!("melt_preset.width", "幅", "Width"),
    e!("melt_preset.depth", "奥行", "Depth"),
    e!("melt_preset.height", "高さ", "Height"),
    e!("melt_preset.keep_base_size", "元形状の大きさを維持", "Keep original size"),
    e!("melt_preset.drip_length", "垂れの長さ (mm)", "Drip length (mm)"),
    e!("melt_preset.drip_thickness", "太さ (mm)", "Thickness (mm)"),
    e!("melt_preset.drip_count", "本数", "Count"),
    e!("melt_preset.drip_tilt", "下向きからの傾き (°)", "Tilt from downward (°)"),
    e!("melt_preset.drip_direction", "方向 (°)", "Direction (°)"),
    e!("melt_preset.viscosity", "粘り・ふくらみ", "Viscosity and bulge"),
    e!("melt_preset.random_seed", "ランダムシード", "Random seed"),
    e!("melt_preset.button_other_drips", "違う垂れ方", "Different drip pattern"),
    e!("melt_preset.thickness_capped", "重なり防止のため太さを {0} mm に制限しています", "Thickness limited to {0} mm to prevent overlap"),
    e!("melt_preset.updating", "更新中…", "Updating…"),
    e!("melt_preset.preparing_preview", "形のプレビューを準備中…", "Preparing shape preview…"),
    e!("melt_preset.sdf_preview", "実際のSDFを使ったプレビュー", "Preview using the actual SDF"),
    e!("melt_preset.hint_smooth_edges", "端は丸く、元の形へ滑らかにつながります", "Edges are rounded and join smoothly to the original shape"),
    e!("melt_preset.hint_curve_edit", "各垂れは通常のCurveとして編集できます", "Each drip can be edited as a regular Curve"),
    e!(
        "melt_preset.warn_melt_zero",
        "シーン全体の「とけぐあい」が0です。接合をなめらかにするにはメルト設定を上げてください",
        "The scene-wide melt amount is 0. Raise the melt setting to smooth the joins"
    ),
    e!(
        "melt_preset.hint_mesh_precision",
        "細い先端を出力する場合はメッシュ精度0.4mm以下を推奨します。",
        "For thin tips in the output, a mesh precision of 0.4 mm or finer is recommended."
    ),
    e!(
        "melt_preset.warn_reapply",
        "再適用すると、このグループの手動変更や追加形状が置き換わります。",
        "Reapplying replaces manual changes and added shapes in this group."
    ),
    e!("melt_preset.button_reapply", "設定を再適用", "Reapply settings"),
    e!("melt_preset.button_add_to_scene", "シーンに追加", "Add to scene"),
    e!("melt_preset.button_cancel", "キャンセル", "Cancel"),
    e!("melt_preset.added_message", "Meltプリセットを追加・更新しました", "Melt preset added or updated"),
    e!("organic_furniture.stool", "まるいスツール", "Round stool"),
    e!("organic_furniture.side_table", "サイドテーブル", "Side table"),
    e!("organic_furniture.pedestal_table", "一本脚テーブル", "Pedestal table"),
    e!("organic_furniture.legs_three", "3本", "3 legs"),
    e!("organic_furniture.legs_four", "4本", "4 legs"),
    e!("organic_furniture.leg_center", "中央1本", "Center leg"),
    e!("organic_furniture.seat", "座面", "Seat"),
    e!("organic_furniture.leg_n", "脚 {0}", "Leg {0}"),
    e!("organic_furniture.leg_joint_n", "脚の付け根 {0}", "Leg joint {0}"),
    e!("organic_furniture.pedestal_base", "一本脚の土台", "Pedestal base"),
    e!("organic_furniture.not_preset", "このグループは家具プリセットではありません", "This group is not a furniture preset"),
    e!(
        "organic_furniture.parts_changed",
        "パーツの順序や数が変わっています。もとの形を残すため、自動更新はしません",
        "Part order or count changed. Auto-update is off to keep the original shape"
    ),
    e!("organic_furniture.limit_groups", "グループは{0}個までです", "Up to {0} groups"),
    e!("organic_furniture.limit_shapes_short", "形は{0}個までです", "Up to {0} shapes"),
    e!("organic_furniture.group_name", "Organic Furniture・{0}", "Organic Furniture・{0}"),
    e!("organic_furniture.heading", "Organic Furniture｜有機家具", "Organic Furniture"),
    e!(
        "organic_furniture.hint_units",
        "数字を動かすと形が変わります。すべてmm単位です。",
        "Change the numbers to reshape the piece. All values are in mm."
    ),
    e!("organic_furniture.design_start", "デザインの出発点", "Design starting point"),
    e!("organic_furniture.leg_count", "脚の本数", "Number of legs"),
    e!("organic_furniture.seat_width", "座面の幅", "Seat width"),
    e!("organic_furniture.seat_depth", "座面の奥行", "Seat depth"),
    e!("organic_furniture.height", "高さ", "Height"),
    e!("organic_furniture.leg_thickness", "脚の太さ", "Leg thickness"),
    e!("organic_furniture.overall_roundness", "全体の丸み", "Overall roundness"),
    e!("organic_furniture.roundness_hint", "0は控えめ、1は角と接合部がなめらか", "0 is subtle, 1 is smooth corners and joints"),
    e!("organic_furniture.puff", "膨張量", "Puff amount"),
    e!(
        "organic_furniture.puff_hint",
        "座面と脚の付け根をぷっくりさせる (mm)。指定幅より少し大きくなります",
        "Puffs the seat and leg joints (mm). Slightly wider than the set width"
    ),
    e!("organic_furniture.symmetric", "左右対称", "Symmetric"),
    e!(
        "organic_furniture.symmetric_hint",
        "オフにすると、脚を少しずらした非対称デザインになります",
        "Turn off for an asymmetric design with slightly offset legs"
    ),
    e!(
        "organic_furniture.blend_note",
        "座面と脚は重ねて滑らかに結合します。パーツ個別の変形は、数値を再変更すると更新されます。",
        "The seat and legs overlap and blend smoothly. Per-part changes update when you edit the numbers again"
    ),
    e!("organic_furniture.limit_shapes_reduce", "形は{0}個までです。ほかの形を減らしてください", "Up to {0} shapes. Remove other shapes first"),
    e!(
        "organic_furniture.safety_note",
        "3Dプリントした形や実物の強度・転倒安全性は保証しません。家具として使う前に別途構造検証が必要です。",
        "We do not guarantee the strength or tip-over safety of the printed shape or the real piece. Check the structure separately before using it as furniture"
    ),
    e!("puffy_text_ui.edit_menu", "Puffy Text の文字・設定を再編集…", "Edit Puffy Text characters and settings…"),
    e!("puffy_text_ui.heading", "Puffy Text  ぷっくり文字", "Puffy Text"),
    e!(
        "puffy_text_ui.regen_note",
        "文字列を再生成します。手動で移動・回転した各文字の位置はできるだけ保持します。",
        "Regenerate the text. Characters you moved or rotated by hand keep their positions where possible."
    ),
    e!(
        "puffy_text_ui.intro",
        "フォントの輪郭から、ふくらんだ3D文字を作ります。日本語・英語に対応。",
        "Make inflated 3D text from font outlines. Supports Japanese and English."
    ),
    e!("puffy_text_ui.text_label", "文字（24文字まで）", "Text (up to 24 characters)"),
    e!("puffy_text_ui.text_hint", "例：ぷっくり TEXT", "e.g. Puffy TEXT"),
    e!(
        "puffy_text_ui.preview_note",
        "プレビュー：アプリUIのフォントです。3D形状は指定フォントで生成します。",
        "Preview: the app UI font. 3D shapes use the chosen font."
    ),
    e!("puffy_text_ui.sample", "Aa あ", "Aa あ"),
    e!("puffy_text_ui.font_label", "使うフォント（TTF・OTF・TTC）", "Font to use (TTF, OTF, TTC)"),
    e!("puffy_text_ui.choose_font", "フォントファイルを選択…", "Choose font file…"),
    e!("puffy_text_ui.find_japanese_font", "日本語フォントを探す", "Find a Japanese font"),
    e!(
        "puffy_text_ui.no_font_found",
        "端末に対応フォントが見つかりません。TTF/OTF/TTCを指定してください",
        "No supported font found on this device. Specify a TTF, OTF or TTC file."
    ),
    e!("puffy_text_ui.font_path_hint", "フォントファイルの場所", "Font file location"),
    e!("puffy_text_ui.face_index", "TTC内の書体番号", "Font face number in TTC"),
    e!("puffy_text_ui.size", "文字サイズ", "Text size"),
    e!("puffy_text_ui.size_hover", "フォントのemサイズ。全体の高さと幅に影響します", "Font em size. Affects the overall height and width."),
    e!("puffy_text_ui.thickness", "文字の厚さ", "Text thickness"),
    e!("puffy_text_ui.bulge", "ふくらみ", "Bulge"),
    e!(
        "puffy_text_ui.bulge_hover",
        "外へ丸くふくらませます。文字の穴がふさがる設定は拒否します",
        "Inflates the letters outward. Settings that would close the holes are rejected."
    ),
    e!("puffy_text_ui.corner_round", "かどの丸み", "Corner rounding"),
    e!("puffy_text_ui.corner_round_hover", "輪郭と表面の角を柔らかくします", "Softens the corners of the outline and surface"),
    e!("puffy_text_ui.letter_spacing", "文字間隔", "Letter spacing"),
    e!("puffy_text_ui.alignment", "位置合わせ", "Alignment"),
    e!(
        "puffy_text_ui.group_note",
        "各文字は別グループです。シーン一覧で文字のグループを選び、ギズモで移動・回転できます。",
        "Each character is a separate group. Pick its group in the scene list and move or rotate it with the gizmo."
    ),
    e!(
        "puffy_text_ui.license_note",
        "フォントファイルは保存・配布しません。立体化した文字の商用利用・配布条件は各フォントのライセンスを確認してください。",
        "Font files are not saved or distributed. Check each font's license for commercial use and distribution of the 3D text."
    ),
    e!("puffy_text_ui.update_button", "文字を更新", "Update text"),
    e!("puffy_text_ui.create_title", "ぷっくり文字を作る", "Make Puffy Text"),
    e!("puffy_text_ui.cancel", "キャンセル", "Cancel"),
    e!("puffy_text_ui.updated", "ぷっくり文字を更新しました", "Puffy Text updated"),
    e!("puffy_text_ui.created", "ぷっくり文字を作りました", "Puffy Text created"),
    e!("chess_dialog.added_piece", "{0}（{1}）を追加しました", "Added {0} ({1})"),
    e!("chess_dialog.select_piece_rebuild", "作り直す駒を選択してください", "Select a piece to rebuild"),
    e!("chess_dialog.rebuilt_selected", "選択した駒を{0}で作り直しました", "Rebuilt the selected piece as {0}"),
    e!(
        "chess_dialog.board_placed",
        "チェス盤と32個の駒を配置しました。各駒は独立したグループです",
        "Placed the chess board and 32 pieces. Each piece is its own group."
    ),
    e!("chess_dialog.title", "Chess Set · チェスの駒", "Chess Set · Chess pieces"),
    e!("chess_dialog.intro", "駒の形を作り、編集できる3Dモデルとして配置します", "Create piece shapes and place them as editable 3D models"),
    e!("chess_dialog.piece_type_label", "駒の種類", "Piece type"),
    e!("chess_dialog.design_label", "デザイン", "Design"),
    e!("chess_dialog.piece_color_label", "駒の色", "Piece color"),
    e!("chess_dialog.height_label", "高さ", "Height"),
    e!("chess_dialog.height_hover", "単体の高さ。32個セットではキングの高さ", "Height of a single piece. In the 32-piece set, the king's height"),
    e!("chess_dialog.base_diameter_label", "底面直径", "Base diameter"),
    e!("chess_dialog.decoration_label", "装飾量", "Decoration amount"),
    e!("chess_dialog.board_width_note", "32駒の並び：8×8マス / 盤の幅 約{0} mm", "32-piece layout: 8×8 squares / board width about {0} mm"),
    e!(
        "chess_dialog.group_note",
        "各駒は独立グループ。選択・複製・パーツ編集・保存できます。",
        "Each piece is an independent group. You can select, duplicate, edit parts, and save."
    ),
    e!(
        "chess_dialog.print_note",
        "3Dプリント：選択した駒ごとに書き出し、プリントチェックを実行してください。",
        "3D print: export each selected piece and run a print check"
    ),
    e!("chess_dialog.add_one_piece", "駒を1個追加", "Add 1 piece"),
    e!("chess_dialog.reedit_selected", "選択した駒を再編集", "Re-edit selected piece"),
    e!(
        "chess_dialog.reedit_hover",
        "選択中のチェス駒を、この設定の形状で置き換えます",
        "Replaces the selected chess piece with the shape of these settings"
    ),
    e!("chess_dialog.place_board_and_pieces", "盤面＋32駒を配置", "Place board + 32 pieces"),
    e!(
        "chess_dialog.place_hover",
        "初期配置。別々の駒として作り、盤面も編集できます",
        "Initial layout. Made as separate pieces; the board can also be edited"
    ),
    e!("chess_dialog.close", "閉じる", "Close"),
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

/// A translated sentence with values put in: `{0}`, `{1}`, ... in the text are replaced by `args`
/// in that order of number, so a translation may move them around.
pub fn fill(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut parts = tr(key).split('{');
    let mut text = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        if let Some((number, tail)) = part.split_once('}')
            && let Some(arg) = number.parse::<usize>().ok().and_then(|i| args.get(i))
        {
            text.push_str(&arg.to_string());
            text.push_str(tail);
        } else {
            text.push('{');
            text.push_str(part);
        }
    }
    text
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
        assert_eq!(keys.len(), 52);
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
                // `{0}`, `{1}`, ... are the numbered places `fill` puts values in; nothing else may have a brace
                let mut plain = value.to_owned();
                for n in 0..10 {
                    plain = plain.replace(&format!("{{{n}}}"), "");
                }
                assert!(!plain.contains('{') && !plain.contains('}'), "{name} {} has a brace placeholder: {value}", entry.key);
                assert!(!["%s", "%d", "%f", "%1$"].iter().any(|p| value.contains(p)), "{name} {} has a percent placeholder: {value}", entry.key);
            }
        }
    }

    #[test]
    fn fill_puts_every_value_in_and_every_language_names_the_same_places() {
        let _en = lang("en");
        assert_eq!(fill("anim.frame", &[&3, &12]), "Frame 3 / 12");
        assert_eq!(fill("anim.frame", &[&"{1}", &"{0}"]), "Frame {1} / {0}", "values must remain literal text");
        assert_eq!(fill("anim.frame", &[&"first"]), "Frame first / {1}", "missing values remain placeholders");
        let _ja = lang("ja");
        assert_eq!(fill("anim.frame", &[&3, &12]), "コマ 3 / 12");
        // a translation may reorder the places, never drop or invent one
        let places = |text: &str| (0..10).filter(|n| text.contains(&format!("{{{n}}}"))).collect::<Vec<_>>();
        for entry in TABLE {
            for other in [entry.ja, entry.zh, entry.ko, entry.de, entry.fr, entry.es] {
                if !other.is_empty() {
                    assert_eq!(places(other), places(entry.en), "{} names other places than the English text", entry.key);
                }
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
