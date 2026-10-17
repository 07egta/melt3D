//! The existing search palette (Ctrl+K or F): type a few letters of a shape, library model, or
//! command, then Enter. It reuses Japanese names and existing English/alternate names.
//! This is a child module of `app` so it can reach the app's private pieces.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use egui::{Key, RichText};
use serde::{Deserialize, Serialize};

use super::{App, ExportFormat, Tool};
use crate::library;
use crate::model::{Kind, Op, Pick, MAX_GROUPS, MAX_SHAPES};
use crate::pattern::{Pattern, PatternKind};
use crate::style;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    AddShape(Kind),
    AddGroup,
    StartSketch,
    StartCurve,
    StartPipe,
    /// (category, item)
    Insert(usize, usize),
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Duplicate,
    MirrorCopy,
    Delete,
    Array,
    /// A live pattern on the selected group.
    Pattern(crate::pattern::PatternKind),
    /// Turn the mirror about world axis 0 / 1 / 2 on or off.
    MirrorAxis(usize),
    ClearPattern,
    /// A linked copy of the selected group.
    MakeInstance,
    NewScene,
    Open,
    Save,
    SaveAs,
    Export(ExportFormat),
    ExportPng,
    PrintCheck,
    ImportSvg,
    ImportMesh,
    FrameAll,
    FrameSelection,
    TogglePerspective,
    FlipOp,
    SetOp(Op),
    SetTool(Tool),
    ViewFront,
    ViewRight,
    ViewTop,
    ShowAssets,
    ShowOutliner,
    Help,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub label: String,
    /// Which kind of thing it is, shown at the right ("形", "ライブラリ", "コマンド").
    pub tag: &'static str,
    /// Lower-case words that also match (English names, readings).
    pub words: String,
    pub action: Action,
}

/// English and common names of a shape kind.
fn kind_words(k: Kind) -> &'static str {
    match k {
        Kind::Sphere => "sphere ball たま きゅう まる",
        Kind::Box => "box cube はこ しかく",
        Kind::Cylinder => "cylinder tube えんちゅう つつ",
        Kind::Torus => "torus donut ring どーなつ わ",
        Kind::Cone => "cone えんすい",
        Kind::Extrude => "extrude おしだし sketch スケッチ",
        Kind::Prism3 => "prism triangle さんかくちゅう",
        Kind::PrismN => "prism polygon hexagon たかくちゅう",
        Kind::Octa => "octahedron はちめんたい",
        Kind::Tetra => "tetrahedron よんめんたい",
        Kind::Pyramid => "pyramid ぴらみっど",
        Kind::Capsule => "capsule pill かぷせる",
        Kind::Hemi => "hemisphere dome はんきゅう",
        Kind::Tube => "pipe tube ぱいぷ",
        Kind::Wedge => "wedge ramp くさび",
        Kind::Star => "star ほし",
        Kind::Revolve => "revolve lathe かいてんたい",
        Kind::Loft => "loft ろふと",
        Kind::Curve => "curve line tube せん かーぶ",
        Kind::Sweep => "sweep すいーぷ",
        Kind::Mesh => "mesh stl obj import めっしゅ",
    }
}

fn cmd(label: &str, words: &str, action: Action) -> Entry {
    Entry { label: label.to_string(), tag: "コマンド", words: words.to_lowercase(), action }
}

/// Everything the palette can run: shapes, every library model, and the commands.
pub fn entries() -> Vec<Entry> {
    let mut v = Vec::new();
    for k in Kind::ALL {
        v.push(Entry { label: format!("{}を足す", k.label()), tag: "形", words: format!("{} {}", k.label(), kind_words(k)).to_lowercase(), action: Action::AddShape(k) });
    }
    for (ci, c) in library::all().iter().enumerate() {
        for (ii, it) in c.items.iter().enumerate() {
            v.push(Entry {
                label: it.name.clone(),
                tag: "ライブラリ",
                words: format!("{} {}", c.name, it.tags.join(" ")).to_lowercase(),
                action: Action::Insert(ci, ii),
            });
        }
    }
    v.extend([
        cmd("グループを作る", "group new まとめる", Action::AddGroup),
        cmd("スケッチを描く", "sketch draw 2d create スケッチ 押し出し", Action::StartSketch),
        cmd("線を描く", "draw line curve stroke せん かーぶ", Action::StartCurve),
        cmd("パイプを描く", "pipe freehand stroke drag ぱいぷ", Action::StartPipe),
        cmd("元に戻す", "undo もどす", Action::Undo),
        cmd("やり直し", "redo", Action::Redo),
        cmd("コピー", "copy", Action::Copy),
        cmd("切り取り", "cut", Action::Cut),
        cmd("貼りつけ", "paste はりつけ", Action::Paste),
        cmd("複製", "duplicate clone ふくせい", Action::Duplicate),
        cmd("左右コピー", "mirror symmetric みらー", Action::MirrorCopy),
        cmd("削除", "delete remove けす さくじょ", Action::Delete),
        cmd("ならべてコピー", "array pattern repeat はいれつ", Action::Array),
        cmd("パターン: 矩形状（たて・よこ・高さにならべる）", "pattern rectangular grid くけいじょう", Action::Pattern(PatternKind::Rect)),
        cmd("パターン: 円形状（ぐるっとならべる）", "pattern circular polar circle えんけいじょう", Action::Pattern(PatternKind::Circular)),
        cmd("パターン: パス上（線にそってならべる）", "pattern path along curve ぱすじょう", Action::Pattern(PatternKind::Path)),
        cmd("パターン: ジオメトリ（点ごとにおく）", "pattern geometry points vertices じおめとり", Action::Pattern(PatternKind::Geometry)),
        cmd("ミラー X（左右にうつす）", "mirror symmetry live x みらー さゆう", Action::MirrorAxis(0)),
        cmd("ミラー Y（前後にうつす）", "mirror symmetry live y みらー ぜんご", Action::MirrorAxis(1)),
        cmd("ミラー Z（上下にうつす）", "mirror symmetry live z みらー じょうげ", Action::MirrorAxis(2)),
        cmd("パターンをやめる", "pattern remove clear off やめる", Action::ClearPattern),
        cmd("インスタンスを作る（リンクコピー）", "instance linked copy いんすたんす りんく", Action::MakeInstance),
        cmd("新しく作る", "new あたらしく", Action::NewScene),
        cmd("開く", "open load ひらく", Action::Open),
        cmd("保存", "save ほぞん", Action::Save),
        cmd("名前をつけて保存", "save as なまえ", Action::SaveAs),
        cmd("STLで書き出す", "export stl print 3dぷりんと かきだし", Action::Export(ExportFormat::Stl)),
        cmd("3MFで書き出す", "export 3mf slicer すらいさー", Action::Export(ExportFormat::ThreeMf)),
        cmd("OBJで書き出す", "export obj", Action::Export(ExportFormat::Obj)),
        cmd("画像(PNG)で書き出す", "export png image screenshot がぞう", Action::ExportPng),
        cmd("プリントチェック", "print check volume size 体積 おおきさ", Action::PrintCheck),
        cmd("SVGを読み込む", "svg import いんぽーと", Action::ImportSvg),
        cmd("メッシュを読み込む", "mesh stl obj 3mf ply glb import めっしゅ よみこむ とりこむ 取り込み", Action::ImportMesh),
        cmd("全体を見る", "frame all home ぜんたい", Action::FrameAll),
        cmd("えらんだものを見る", "frame focus selection みる", Action::FrameSelection),
        cmd("遠近 / 平行を切りかえ", "perspective orthographic ortho えんきん へいこう", Action::TogglePerspective),
        cmd("足す ⇄ 削る を切りかえ", "flip boolean subtract add tab", Action::FlipOp),
        cmd("つぎは「足す」", "add union", Action::SetOp(Op::Add)),
        cmd("つぎは「削る」", "subtract difference cut けずる", Action::SetOp(Op::Subtract)),
        cmd("つぎは「ぬる」", "paint stain ぬる", Action::SetOp(Op::Paint)),
        cmd("つぎは「重ねる」", "intersect かさねる", Action::SetOp(Op::Intersect)),
        cmd("道具: 選ぶ", "tool select q", Action::SetTool(Tool::Select)),
        cmd("道具: ぜんぶ", "tool all gizmo a", Action::SetTool(Tool::All)),
        cmd("道具: 動かす", "tool move translate w", Action::SetTool(Tool::Move)),
        cmd("道具: 回す", "tool rotate e", Action::SetTool(Tool::Rotate)),
        cmd("道具: 大きさ", "tool scale r", Action::SetTool(Tool::Scale)),
        cmd("正面から見る", "view front 1 しょうめん", Action::ViewFront),
        cmd("右から見る", "view right 3 みぎ", Action::ViewRight),
        cmd("真上から見る", "view top 7 うえ", Action::ViewTop),
        cmd("ライブラリを開く", "library assets らいぶらり あせっと", Action::ShowAssets),
        cmd("シーン一覧を開く", "outliner scene しーん あうとらいなー", Action::ShowOutliner),
        cmd("操作のしかた", "help keys keyboard へるぷ ?", Action::Help),
    ]);
    v
}

/// Existing labels and English/Japanese aliases above remain the single source of truth.
/// Index their 1-3 character substrings once; on a query, score only the posting-list
/// intersection rather than examining every shape and library item.
fn gram_keys(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(width).map(|window| format!("{width}:{}", window.iter().collect::<String>())).collect()
}

fn action_key(action: Action) -> String {
    // Models are identified by their category and label, not mutable row indices.
    // Other commands use their existing enum identity, independent of UI translation.
    match action {
        Action::Insert(cat, item) => {
            if let Some(category) = library::all().get(cat)
                && let Some(model) = category.items.get(item)
            {
                return format!("library:{}:{}", category.name, model.name);
            }
            format!("library:missing:{cat}:{item}")
        }
        _ => format!("{action:?}"),
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Prefs {
    favorites: Vec<String>,
    recent: Vec<String>,
}

impl Prefs {
    const RECENT_LIMIT: usize = 16;
    const FAVORITE_LIMIT: usize = 128;

    fn path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            Some(PathBuf::from(std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"))?).join("YuraCAD").join("command_palette.json"))
        }
        #[cfg(target_os = "macos")]
        {
            Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/YuraCAD/command_palette.json"))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let root = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
            Some(root.join("yuracad").join("command_palette.json"))
        }
    }

    pub(super) fn load() -> Self {
        if cfg!(test) { return Self::default(); }
        let Some(path) = Self::path() else { return Self::default() };
        std::fs::read_to_string(path).ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
            .sanitized()
    }

    fn sanitized(mut self) -> Self {
        fn unique(v: &mut Vec<String>, limit: usize) {
            let mut seen = HashSet::new();
            v.retain(|s| seen.insert(s.clone()));
            v.truncate(limit);
        }
        unique(&mut self.favorites, Self::FAVORITE_LIMIT);
        unique(&mut self.recent, Self::RECENT_LIMIT);
        self
    }

    fn save(&self) {
        if cfg!(test) { return; }
        let Some(path) = Self::path() else { return };
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() { return; }
        if let Ok(data) = serde_json::to_vec_pretty(self) {
            // Preferences are independent of user project files. Failed persistence must
            // never interrupt modelling; an unreadable file simply loads defaults.
            let _ = std::fs::write(path, data);
        }
    }

    fn contains(&self, action: Action) -> bool {
        self.favorites.contains(&action_key(action))
    }

    fn toggle(&mut self, action: Action) {
        let key = action_key(action);
        if let Some(i) = self.favorites.iter().position(|s| s == &key) {
            self.favorites.remove(i);
        } else if self.favorites.len() < Self::FAVORITE_LIMIT {
            self.favorites.push(key);
        }
    }

    fn record(&mut self, action: Action) {
        let key = action_key(action);
        self.recent.retain(|s| s != &key);
        self.recent.insert(0, key);
        self.recent.truncate(Self::RECENT_LIMIT);
    }
}

struct Catalog {
    entries: Vec<Entry>,
    keys: Vec<String>,
    folded_labels: Vec<String>,
    by_key: HashMap<String, usize>,
    postings: HashMap<String, Vec<usize>>,
    /// Computed only once, rather than finding default commands on every open.
    defaults: Vec<usize>,
}

impl Catalog {
    fn new(entries: Vec<Entry>) -> Self {
        let mut postings: HashMap<String, Vec<usize>> = HashMap::new();
        let mut keys = Vec::with_capacity(entries.len());
        let mut folded_labels = Vec::with_capacity(entries.len());
        let mut by_key = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            let key = action_key(e.action);
            by_key.insert(key.clone(), i);
            keys.push(key);
            let folded_label = e.label.to_lowercase();
            folded_labels.push(folded_label.clone());
            // Keep label and aliases separate: do not create imaginary substrings
            // crossing from the Japanese name into its English aliases.
            for text in [folded_label, e.words.to_lowercase()] {
                for width in 1..=3 {
                    for gram in gram_keys(&text, width) {
                        let list = postings.entry(gram).or_default();
                        if list.last() != Some(&i) { list.push(i); }
                    }
                }
            }
        }
        let mut defaults = Vec::new();
        for action in [
            Action::AddShape(Kind::Box),
            Action::AddShape(Kind::Sphere),
            Action::Save,
            Action::StartSketch,
            Action::StartCurve,
            Action::StartPipe,
            Action::ShowAssets,
            Action::ImportMesh,
            Action::SetTool(Tool::Move),
            Action::Undo,
            Action::Export(ExportFormat::Stl),
            Action::Help,
        ] {
            if let Some(&i) = by_key.get(&action_key(action)) { defaults.push(i); }
        }
        for (i, e) in entries.iter().enumerate() {
            if e.tag != "ライブラリ" && !defaults.contains(&i) { defaults.push(i); }
        }
        Self { entries, keys, folded_labels, by_key, postings, defaults }
    }

    /// Intersection of postings for all query n-grams. No full catalog iteration.
    fn candidates_at_width(&self, query: &str, forced_width: Option<usize>) -> Vec<usize> {
        let tokens: Vec<&str> = query.split_whitespace().collect();
        if tokens.is_empty() { return Vec::new(); }
        let mut lists: Vec<&Vec<usize>> = Vec::new();
        for token in tokens {
            let n = forced_width.unwrap_or_else(|| token.chars().count().min(3));
            for gram in gram_keys(token, n) {
                let Some(list) = self.postings.get(&gram) else { return Vec::new() };
                lists.push(list);
            }
        }
        lists.sort_by_key(|v| v.len());
        let Some(first) = lists.first() else { return Vec::new() };
        let mut ids = first.to_vec();
        for list in lists.iter().skip(1) {
            ids.retain(|i| list.binary_search(i).is_ok());
            if ids.is_empty() { break; }
        }
        ids
    }

    fn candidates(&self, query: &str) -> Vec<usize> {
        self.candidates_at_width(query, None)
    }

    fn search(&self, query: &str, prefs: &Prefs, limit: usize) -> Vec<usize> {
        if limit == 0 { return Vec::new(); }
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            let mut out = Vec::with_capacity(limit);
            let mut seen = HashSet::new();
            for key in prefs.favorites.iter().chain(prefs.recent.iter()) {
                if let Some(&i) = self.by_key.get(key) {
                    if seen.insert(i) { out.push(i); }
                    if out.len() == limit { return out; }
                }
            }
            for &i in &self.defaults {
                if seen.insert(i) { out.push(i); }
                if out.len() == limit { break; }
            }
            return out;
        }
        let mut candidates = self.candidates(&q);
        let mut scored = self.rank_candidates(&candidates, &q, prefs, limit);
        // Retain the old initials / ordered-letters search without the old full scan:
        // only try intersecting per-character postings if no substring matches.
        if scored.is_empty() && !q.contains(char::is_whitespace) && q.chars().count() > 1 {
            candidates = self.candidates_at_width(&q, Some(1));
            scored = self.rank_candidates(&candidates, &q, prefs, limit);
        }
        scored.into_iter().map(|(_, i)| i).collect()
    }

    /// Bounded top-k heap: one-letter queries may have many candidates, but sorting
    /// stays O(matches * log(limit)), not O(all_entries * log(all_entries)).
    fn rank_candidates(&self, ids: &[usize], q: &str, prefs: &Prefs, limit: usize) -> Vec<((u32, usize, usize, usize, usize), usize)> {
        let favorite: HashSet<&str> = prefs.favorites.iter().map(String::as_str).collect();
        let recent: HashMap<&str, usize> = prefs.recent.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();
        let mut best = BinaryHeap::new();
        for &i in ids {
            let Some(score) = score(&self.entries[i], &self.folded_labels[i], q) else { continue };
            let key = self.keys[i].as_str();
            let rank = (
                score,
                usize::from(self.entries[i].tag != "コマンド"),
                usize::from(!favorite.contains(key)),
                recent.get(key).copied().unwrap_or(usize::MAX),
                i,
            );
            if best.len() < limit {
                best.push((rank, i));
            } else if best.peek().is_some_and(|(worst, _)| rank < *worst) {
                best.pop();
                best.push((rank, i));
            }
        }
        let mut hits = best.into_vec();
        hits.sort_unstable_by_key(|(rank, _)| *rank);
        hits
    }
}

/// How well q matches an existing entry. Exact/partial names outrank aliases;
/// ties prefer executable commands over library models. Lower is better.
fn score(e: &Entry, label: &str, q: &str) -> Option<u32> {
    if label == q { return Some(0); }
    if label.starts_with(q) { return Some(1); }
    if label.contains(q) { return Some(2); }
    if e.words.split_whitespace().any(|w| w == q) { return Some(3); }
    if e.words.split_whitespace().any(|w| w.starts_with(q)) { return Some(4); }
    if e.words.contains(q) { return Some(5); }
    let mut it = label.chars().chain(std::iter::once(' ')).chain(e.words.chars());
    if q.chars().all(|c| it.any(|x| x == c)) { return Some(6); }
    None
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| Catalog::new(entries()))
}

#[derive(Clone, Default)]
struct State {
    query: String,
    sel: usize,
    fresh: bool,
    hits: Vec<usize>,
    dirty: bool,
}

fn id() -> egui::Id {
    egui::Id::new("palette_state")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), State { fresh: true, dirty: true, ..State::default() }));
}

pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<State>(id())).is_some()
}

impl App {
    /// An unusable result stays visible, with a reason, but cannot be executed.
    /// Only the few visible hits (max 9) are checked, never the full catalog.
    fn palette_disabled_reason(&self, a: Action) -> Option<&'static str> {
        let selected = match self.sel {
            Some(Pick::Shape(g, s)) => self.scene.shape(g, s).is_some(),
            Some(Pick::Group(g)) => g < self.scene.groups.len(),
            None => false,
        };
        let group = self.selected_group();
        match a {
            Action::Undo if self.undo.is_empty() => Some("元に戻す操作がありません"),
            Action::Redo if self.redo.is_empty() => Some("やり直す操作がありません"),
            Action::Copy | Action::Cut | Action::Delete | Action::Array | Action::FrameSelection if !selected => Some("先に形かグループを選択してください"),
            Action::Paste if self.clipboard.is_none() => Some("コピーしたデータがありません"),
            Action::Duplicate if !selected => Some("先に形かグループを選択してください"),
            Action::Duplicate if self.scene.shape_count() >= MAX_SHAPES => Some("形の最大数に達しています"),
            Action::Duplicate if matches!(self.sel, Some(Pick::Group(_))) && self.scene.groups.len() >= MAX_GROUPS => Some("グループの最大数に達しています"),
            Action::Duplicate if matches!(self.sel, Some(Pick::Group(g)) if self.scene.groups.get(g).is_some_and(|group| self.scene.shape_count() + group.shapes.len() > MAX_SHAPES)) => {
                Some("グループ全体を複製すると形の上限を超えます")
            },
            Action::MirrorCopy if !matches!(self.sel, Some(Pick::Shape(_, _))) => Some("左右コピーは形を1つ選択してください"),
            Action::MirrorCopy if self.scene.shape_count() >= MAX_SHAPES => Some("形の最大数に達しています"),
            Action::AddShape(_) | Action::StartSketch | Action::StartCurve | Action::StartPipe if self.scene.shape_count() >= MAX_SHAPES => Some("形の最大数に達しています"),
            Action::StartSketch if self.sketch_active() => Some("スケッチを編集中です"),
            Action::StartCurve if self.ct.drawing => Some("線の描画ツールは使用中です"),
            Action::StartPipe if self.pt.on => Some("パイプの描画ツールは使用中です"),
            Action::AddGroup if self.scene.groups.len() >= MAX_GROUPS => Some("グループの最大数に達しています"),
            Action::Insert(c, i) => {
                match library::all().get(c).and_then(|category| category.items.get(i)) {
                    None => Some("このライブラリ項目は利用できません"),
                    Some(item) if !item.fits(self.scene.groups.len(), self.scene.shape_count()) => Some("追加すると形またはグループの上限を超えます"),
                    _ => None,
                }
            }
            Action::Pattern(_) | Action::MirrorAxis(_) | Action::ClearPattern | Action::MakeInstance if group.is_none() => {
                Some("先に形かグループを選択してください")
            }
            Action::Pattern(PatternKind::Path) if !self.scene.groups.iter().any(|g| g.shapes.iter().any(|s| crate::pattern::shape_polyline(s).is_some())) => {
                Some("パスに使う線や輪郭がありません")
            }
            Action::Pattern(PatternKind::Geometry) if !self.scene.groups.iter().enumerate().any(|(gi, g)| {
                g.shapes.iter().any(|s| crate::pattern::shape_polyline(s).is_some())
                    || (Some(gi) != group && !g.shapes.is_empty())
            }) => {
                Some("点を参照する形がありません")
            }
            Action::MirrorAxis(_) if group.and_then(|g| self.scene.groups.get(g)).and_then(|g| g.pattern.as_ref()).is_some_and(|p| !matches!(p, Pattern::Mirror { .. })) => {
                Some("別のパターンを先に解除してください")
            }
            Action::ClearPattern if !group.and_then(|g| self.scene.groups.get(g)).is_some_and(|g| g.pattern.is_some()) => {
                Some("解除するパターンがありません")
            }
            Action::MakeInstance if self.scene.groups.len() >= MAX_GROUPS => Some("グループの最大数に達しています"),
            Action::Export(_) | Action::ExportPng | Action::PrintCheck if self.scene.shape_count() == 0 => Some("先に形を作ってください"),
            Action::NewScene | Action::Open if self.pending_file_action.is_some() => Some("保存確認を先に完了してください"),
            _ => None,
        }
    }

    /// Do what a palette entry says.
    pub fn run_action(&mut self, ctx: &egui::Context, a: Action) {
        let pi = std::f32::consts::PI;
        match a {
            Action::AddShape(k) => self.add_shape(ctx, k),
            Action::AddGroup => self.add_group(),
            Action::StartSketch => self.set_sketch_tool(true),
            Action::StartCurve => self.set_curve_tool(true),
            Action::StartPipe => self.set_pipe_tool(true),
            Action::Insert(c, i) => {
                if let Some(item) = library::all().get(c).and_then(|c| c.items.get(i)) {
                    crate::library_ui::insert(self, ctx, item);
                }
            }
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Copy => self.copy(ctx),
            Action::Cut => self.cut(ctx),
            Action::Paste => self.paste(ctx, None),
            Action::Duplicate => self.duplicate(),
            Action::MirrorCopy => self.mirror_copy(),
            Action::Delete => self.delete_selected(),
            Action::Array => crate::array::open(ctx),
            Action::Pattern(k) => self.set_pattern_kind(ctx, k),
            Action::MirrorAxis(a) => self.toggle_mirror(ctx, a),
            Action::ClearPattern => self.clear_pattern(),
            Action::MakeInstance => self.make_instance(ctx),
            Action::NewScene => {
                self.new_scene(crate::model::Scene::default());
            }
            Action::Open => self.open_dialog(ctx),
            Action::Save => self.save(ctx, false),
            Action::SaveAs => self.save(ctx, true),
            Action::Export(f) => self.export_as(ctx, f),
            Action::ExportPng => self.start_png(ctx),
            Action::PrintCheck => self.print_check(ctx),
            Action::ImportSvg => crate::svg_dialog::open(ctx),
            Action::ImportMesh => crate::mesh_dialog::open(ctx),
            Action::FrameAll => self.frame_view(false),
            Action::FrameSelection => self.frame_view(true),
            Action::TogglePerspective => self.cam.ortho = !self.cam.ortho,
            Action::FlipOp => self.flip_op(),
            Action::SetOp(o) => self.new_op = o,
            Action::SetTool(t) => self.tool = t,
            Action::ViewFront => self.fly_look(0.0, 0.0),
            Action::ViewRight => self.fly_look(pi / 2.0, 0.0),
            Action::ViewTop => self.fly_look(self.cam.yaw, pi / 2.0),
            Action::ShowAssets => self.lib_open = true,
            Action::ShowOutliner => self.lib_open = false,
            Action::Help => self.show_keys = !self.show_keys,
        }
    }

    /// One keyboard-accessible palette, reused by F and Ctrl+K.
    pub fn palette_ui(&mut self, ctx: &egui::Context) {
        let Some(mut st) = ctx.data(|m| m.get_temp::<State>(id())) else { return };
        let cat = catalog();
        let mut close = false;
        let (up, down, enter, favorite_key) = ctx.input_mut(|i| {
            close = i.consume_key(egui::Modifiers::NONE, Key::Escape)
                || i.consume_key(egui::Modifiers::COMMAND, Key::K);
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::ALT, Key::F),
            )
        });
        if close {
            ctx.data_mut(|m| m.remove::<State>(id()));
            return;
        }
        if st.dirty {
            st.hits = cat.search(&st.query, &self.palette_prefs, 9);
            st.sel = st.sel.min(st.hits.len().saturating_sub(1));
            st.dirty = false;
        }

        let mut execute = None;
        let mut toggle_favorite = None;
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("palette_area"))
            .order(egui::Order::Tooltip)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(rect.center().x, rect.top() + 70.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_width((rect.width() - 24.0).clamp(240.0, 520.0));
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut st.query)
                            .hint_text("コマンド・形・ライブラリを検索 / Search commands")
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::proportional(15.0))
                    );
                    if st.fresh {
                        edit.request_focus();
                        st.fresh = false;
                    }
                    if edit.changed() {
                        st.sel = 0;
                        st.dirty = true;
                    }
                    // Re-indexed only when the query or favorite order changes, not every frame.
                    if st.dirty {
                        st.hits = cat.search(&st.query, &self.palette_prefs, 9);
                        st.sel = st.sel.min(st.hits.len().saturating_sub(1));
                        st.dirty = false;
                    }
                    if up { st.sel = st.sel.saturating_sub(1); }
                    if down { st.sel = (st.sel + 1).min(st.hits.len().saturating_sub(1)); }
                    if enter { execute = st.hits.get(st.sel).copied(); }
                    if favorite_key {
                        toggle_favorite = st.hits.get(st.sel)
                            .and_then(|&i| cat.entries.get(i)).map(|e| e.action);
                    }

                    ui.add_space(5.0);
                    if st.hits.is_empty() {
                        ui.label(RichText::new("見つかりませんでした / No results").color(style::DIM));
                    }
                    let visible = st.hits.clone(); // at most 9 small indices
                    for (row, i) in visible.into_iter().enumerate() {
                        let Some(e) = cat.entries.get(i) else { continue };
                        let reason = self.palette_disabled_reason(e.action);
                        let favorite = self.palette_prefs.contains(e.action);
                        ui.horizontal(|ui| {
                            let star = if favorite { "★" } else { "☆" };
                            if ui.small_button(star).on_hover_text("お気に入り切替 (Alt+F)").clicked() {
                                toggle_favorite = Some(e.action);
                            }
                            let label = if let Some(why) = reason {
                                RichText::new(&e.label).color(style::DIM)
                                    .size(13.5)
                            } else {
                                RichText::new(&e.label).color(style::TEXT)
                                    .size(13.5)
                            };
                            if ui.selectable_label(row == st.sel, label)
                                .on_hover_text(reason.unwrap_or("Enter / クリックで実行"))
                                .clicked()
                            {
                                st.sel = row;
                                execute = Some(i);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let section = if st.query.trim().is_empty() && self.palette_prefs.recent.contains(&cat.keys[i]) {
                                    "最近"
                                } else {
                                    e.tag
                                };
                                ui.label(RichText::new(section).size(11.0).color(style::DIM));
                            });
                        });
                        if let Some(why) = reason {
                            ui.horizontal(|ui| {
                                ui.add_space(28.0);
                                ui.label(RichText::new(why).size(11.0).color(style::DIM));
                            });
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("↑↓ 選択  Enter 実行  Alt+F ★登録  Esc 閉じる").size(11.0).color(style::DIM));
                });
            });

        if let Some(action) = toggle_favorite {
            self.palette_prefs.toggle(action);
            self.palette_prefs.save();
            // Mark dirty and refresh next frame (do not traverse the index every repaint).
            st.dirty = true;
        }
        if let Some(i) = execute {
            if let Some(e) = cat.entries.get(i) {
                if let Some(reason) = self.palette_disabled_reason(e.action) {
                    self.say(ctx, reason);
                } else {
                    let action = e.action;
                    self.run_action(ctx, action);
                    self.palette_prefs.record(action);
                    self.palette_prefs.save();
                    close = true;
                }
            }
        }
        if close {
            ctx.data_mut(|m| m.remove::<State>(id()));
        } else {
            ctx.data_mut(|m| m.insert_temp(id(), st));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(q: &str) -> Option<Action> {
        let cat = catalog();
        cat.search(q, &Prefs::default(), 5).first().and_then(|i| cat.entries.get(*i)).map(|e| e.action)
    }

    #[test]
    fn japanese_and_english_names_find_the_same_thing() {
        assert_eq!(top("sphere"), Some(Action::AddShape(Kind::Sphere)));
        assert_eq!(top("sketch"), Some(Action::StartSketch));
        assert_eq!(top("パイプ"), Some(Action::StartPipe));
        assert_eq!(top("球"), Some(Action::AddShape(Kind::Sphere)));
        assert_eq!(top("たま"), Some(Action::AddShape(Kind::Sphere)));
        assert_eq!(top("undo"), Some(Action::Undo));
        assert_eq!(top("元に戻す"), Some(Action::Undo));
        assert_eq!(top("stl"), Some(Action::Export(ExportFormat::Stl)));
        assert_eq!(top("array"), Some(Action::Array));
        assert_eq!(top("ならべ"), Some(Action::Array));
    }

    #[test]
    fn library_models_are_found_by_name_and_tag() {
        let cat = catalog();
        let all = &cat.entries;
        assert!(all.iter().any(|e| matches!(e.action, Action::Insert(_, _))), "the library is in the palette");
        let first = library::all().first().and_then(|c| c.items.first()).map(|i| i.name.clone()).unwrap_or_default();
        let hits = cat.search(&first, &Prefs::default(), 5);
        assert!(hits.iter().any(|i| all.get(*i).is_some_and(|e| e.label == first)));
    }

    #[test]
    fn empty_and_nonsense_queries_are_safe() {
        let cat = catalog();
        let p = Prefs::default();
        let n = cat.search("", &p, 9).len();
        assert!(n > 0 && n <= 9);
        assert!(cat.search("zzzzqqqqxxxx😀", &p, 9).is_empty());
        assert!(cat.search("   ", &p, 3).len() <= 3);
    }

    #[test]
    fn best_matches_come_first() {
        let cat = catalog();
        let hits = cat.search("コピー", &Prefs::default(), 20);
        let labels: Vec<&str> = hits.iter().filter_map(|i| cat.entries.get(*i)).map(|e| e.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"コピー"), "{labels:?}");
    }


    #[test]
    fn indexed_substrings_and_bilingual_aliases_match_without_scanning_every_entry() {
        let cat = catalog();
        assert_eq!(top("phere"), Some(Action::AddShape(Kind::Sphere)));
        assert_eq!(top("やり直"), Some(Action::Redo));
        assert_eq!(top("plicate"), Some(Action::Duplicate));
        assert_eq!(top("書き出"), Some(Action::Export(ExportFormat::Stl)));
        assert!(cat.search("sPhErE", &Prefs::default(), 5).iter().any(|&i| cat.entries[i].action == Action::AddShape(Kind::Sphere)));

        // Many commands: a rare substring visits only a short posting list.
        let mut entries = Vec::new();
        for n in 0..8000 {
            entries.push(cmd(&format!("feature_{n:05}_処理"), "generic action", Action::Help));
        }
        let big = Catalog::new(entries);
        let q = "feature_07999";
        let candidates = big.candidates(q);
        assert_eq!(candidates.len(), 1);
        assert_eq!(big.search(q, &Prefs::default(), 9).len(), 1);
        assert_eq!(big.search("generic", &Prefs::default(), 9).len(), 9, "broad queries still have a bounded top-k result");
        assert!(big.candidates("不明な機能").is_empty());
        // Record observed p95 without a brittle wall-clock pass/fail threshold.
        // The structural assertion above guarantees narrow posting-list lookup.
        let prefs = Prefs::default();
        let mut micros = Vec::with_capacity(250);
        for _ in 0..250 {
            let started = std::time::Instant::now();
            let found = big.search(q, &prefs, 9);
            assert_eq!(found.len(), 1);
            micros.push(started.elapsed().as_micros());
        }
        micros.sort_unstable();
        eprintln!("command palette: 8,000 entries, query={q}, p95={} us", micros[micros.len() * 95 / 100]);
    }

    #[test]
    fn favorite_recent_and_missing_ids_have_a_stable_order_and_round_trip() {
        let cat = catalog();
        let mut prefs = Prefs::default();
        prefs.record(Action::Undo);
        prefs.record(Action::Save);
        prefs.toggle(Action::ImportSvg);
        prefs.toggle(Action::Save);
        let hits = cat.search("", &prefs, 9);
        assert_eq!(cat.entries[hits[0]].action, Action::ImportSvg);
        assert_eq!(cat.entries[hits[1]].action, Action::Save);
        assert_eq!(cat.entries[hits[2]].action, Action::Undo);
        prefs.toggle(Action::Save);
        assert!(!prefs.contains(Action::Save));
        let encoded = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<Prefs>(&encoded).unwrap(), prefs);
        prefs.favorites.insert(0, "RemovedOldCommand".into());
        assert_eq!(cat.entries[cat.search("", &prefs, 9)[0]].action, Action::ImportSvg);
        for _ in 0..30 { prefs.record(Action::Delete); }
        assert_eq!(prefs.recent.iter().filter(|x| *x == &action_key(Action::Delete)).count(), 1);
        assert!(prefs.recent.len() <= Prefs::RECENT_LIMIT);
    }

    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key, physical_key: None, pressed: true, repeat: false, modifiers,
        }
    }

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            app.keys(ui.ctx());
            app.palette_ui(ui.ctx());
        });
        out.textures_delta.clear();
    }

    #[test]
    fn keyboard_ctrl_k_arrows_enter_escape_and_shortcut_isolation() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.palette_prefs = Prefs::default();
        app.tool = Tool::Select;
        frame(&ctx, &mut app, vec![key(Key::K, egui::Modifiers::COMMAND)]);
        assert!(is_open(&ctx), "Ctrl+K opens the existing palette");
        let mut state = ctx.data(|m| m.get_temp::<State>(id())).unwrap();
        state.query = "道具:".into();
        state.dirty = true;
        state.fresh = false;
        ctx.data_mut(|m| m.insert_temp(id(), state));
        frame(&ctx, &mut app, vec![]);
        let first = ctx.data(|m| m.get_temp::<State>(id())).unwrap();
        assert!(first.hits.len() >= 3, "multiple tool commands are searchable");
        frame(&ctx, &mut app, vec![key(Key::ArrowDown, egui::Modifiers::NONE)]);
        let second = ctx.data(|m| m.get_temp::<State>(id())).unwrap();
        assert_eq!(second.sel, 1, "down moves only palette selection");
        assert_eq!(app.tool, Tool::Select, "down did not operate a viewport shortcut");
        frame(&ctx, &mut app, vec![key(Key::ArrowUp, egui::Modifiers::NONE)]);
        assert_eq!(ctx.data(|m| m.get_temp::<State>(id())).unwrap().sel, 0, "up moves palette selection");
        frame(&ctx, &mut app, vec![key(Key::ArrowDown, egui::Modifiers::NONE)]);
        let second = ctx.data(|m| m.get_temp::<State>(id())).unwrap();
        let action = catalog().entries[second.hits[second.sel]].action;
        assert!(matches!(action, Action::SetTool(_)));
        frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert!(!is_open(&ctx), "Enter executed the selected result and closed");
        if let Action::SetTool(tool) = action {
            assert_eq!(app.tool, tool);
        }
        assert_eq!(app.palette_prefs.recent.first(), Some(&action_key(action)));
        frame(&ctx, &mut app, vec![key(Key::K, egui::Modifiers::COMMAND)]);
        assert!(is_open(&ctx));
        frame(&ctx, &mut app, vec![key(Key::F, egui::Modifiers::ALT)]);
        let favorite_action = catalog().entries[ctx.data(|m| m.get_temp::<State>(id())).unwrap().hits[0]].action;
        assert!(app.palette_prefs.contains(favorite_action), "Alt+F registers a favorite without executing");
        frame(&ctx, &mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
        assert!(!is_open(&ctx));
    }

    #[test]
    fn unavailable_result_shows_reason_and_enter_does_not_execute() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.palette_prefs = Prefs::default();
        app.sel = None;
        assert_eq!(app.palette_disabled_reason(Action::Duplicate), Some("先に形かグループを選択してください"));
        assert_eq!(app.palette_disabled_reason(Action::Undo), Some("元に戻す操作がありません"));
        open(&ctx);
        let mut st = ctx.data(|m| m.get_temp::<State>(id())).unwrap();
        st.query = "duplicate".into();
        st.dirty = true;
        ctx.data_mut(|m| m.insert_temp(id(), st));
        frame(&ctx, &mut app, vec![]);
        let before = app.scene.shape_count();
        frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert!(is_open(&ctx));
        assert_eq!(app.scene.shape_count(), before);
        assert!(app.palette_prefs.recent.is_empty());
        app.sel = Some(Pick::Shape(0, 0));
        frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(app.scene.shape_count(), before + 1);
        assert!(!is_open(&ctx));
    }

    #[test]
    fn palette_opens_runs_and_closes() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        assert!(!is_open(&ctx));
        open(&ctx);
        assert!(is_open(&ctx));
        ctx.run_ui(egui::RawInput::default(), |ui| app.palette_ui(ui.ctx())).textures_delta.clear();
        app.run_action(&ctx, Action::AddShape(Kind::Box));
        assert_eq!(app.scene.shape_count(), 2);
        app.run_action(&ctx, Action::Duplicate);
        app.run_action(&ctx, Action::Undo);
        app.run_action(&ctx, Action::ShowAssets);
        assert!(app.lib_open);
        // every action can run on a fresh app without panicking (dialogs excluded: they need a window)
        for e in entries() {
            if matches!(e.action, Action::Open | Action::Save | Action::SaveAs | Action::Export(_) | Action::ExportPng | Action::ImportSvg | Action::ImportMesh) {
                continue;
            }
            let mut a = App::new(true, None);
            a.run_action(&ctx, e.action);
        }
    }
}
