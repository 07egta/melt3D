//! The search palette (key F): type a few letters of anything - a shape, a ready-made model, a
//! command - and press Enter. It matches Japanese names and English / other common names.
//! This is a child module of `app` so it can reach the app's private pieces.

use egui::{Align2, Color32, Key, RichText, Vec2};

use super::{App, ExportFormat, Tool};
use crate::library;
use crate::model::{Kind, Op};
use crate::pattern::PatternKind;
use crate::style;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    AddShape(Kind),
    AddGroup,
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
        Kind::Sphere => "sphere ball たま きゅう まる 球",
        Kind::Box => "box cube はこ しかく 箱",
        Kind::Cylinder => "cylinder tube えんちゅう つつ 円柱",
        Kind::Torus => "torus donut ring どーなつ わ ドーナツ",
        Kind::Cone => "cone えんすい 円すい",
        Kind::Extrude => "extrude おしだし sketch スケッチ 押し出し",
        Kind::Prism3 => "prism triangle さんかくちゅう 三角柱",
        Kind::PrismN => "prism polygon hexagon たかくちゅう 多角柱",
        Kind::Octa => "octahedron はちめんたい 正八面体",
        Kind::Tetra => "tetrahedron よんめんたい 正四面体",
        Kind::Pyramid => "pyramid ぴらみっど ピラミッド",
        Kind::Capsule => "capsule pill かぷせる カプセル",
        Kind::Hemi => "hemisphere dome はんきゅう 半球",
        Kind::Tube => "pipe tube ぱいぷ パイプ",
        Kind::Wedge => "wedge ramp くさび",
        Kind::Star => "star ほし 星柱",
        Kind::Revolve => "revolve lathe かいてんたい 回転体",
        Kind::Loft => "loft ろふと ロフト",
        Kind::Curve => "curve line tube せん かーぶ 線",
        Kind::Sweep => "sweep すいーぷ スイープ",
        Kind::Mesh => "mesh stl obj import めっしゅ メッシュ",
    }
}

fn cmd(label: &str, words: &str, action: Action) -> Entry {
    Entry { label: label.to_string(), tag: "Command", words: words.to_lowercase(), action }
}

/// Everything the palette can run: shapes, every library model, and the commands.
pub fn entries() -> Vec<Entry> {
    let mut v = Vec::new();
    for k in Kind::ALL {
        v.push(Entry {
            label: format!("Add {}", k.label()),
            tag: "Shape",
            words: format!("{} {}", k.label(), kind_words(k)).to_lowercase(),
            action: Action::AddShape(k),
        });
    }
    for (ci, c) in library::all().iter().enumerate() {
        for (ii, it) in c.items.iter().enumerate() {
            v.push(Entry {
                label: it.name.clone(),
                tag: "Library",
                words: format!("{} {}", c.name, it.tags.join(" ")).to_lowercase(),
                action: Action::Insert(ci, ii),
            });
        }
    }
    v.extend([
        cmd("Group", "group new まとめる グループを作る", Action::AddGroup),
        cmd("Undo", "undo もどす 元に戻す", Action::Undo),
        cmd("Redo", "redo やり直し", Action::Redo),
        cmd("Copy", "copy コピー", Action::Copy),
        cmd("Cut", "cut 切り取り", Action::Cut),
        cmd("Paste", "paste はりつけ 貼りつけ", Action::Paste),
        cmd("Duplicate", "duplicate clone ふくせい 複製", Action::Duplicate),
        cmd("Mirror Copy", "mirror symmetric みらー 左右コピー", Action::MirrorCopy),
        cmd("Delete", "delete remove けす さくじょ 削除", Action::Delete),
        cmd("Array", "array pattern repeat はいれつ ならべてコピー", Action::Array),
        cmd("Pattern: Rectangular", "pattern rectangular grid くけいじょう パターン 矩形状 ならべる", Action::Pattern(PatternKind::Rect)),
        cmd("Pattern: Circular", "pattern circular polar circle えんけいじょう パターン 円形状 ぐるっと", Action::Pattern(PatternKind::Circular)),
        cmd("Pattern: Along Path", "pattern path along curve ぱすじょう パターン パス上 線にそって", Action::Pattern(PatternKind::Path)),
        cmd("Pattern: Points", "pattern geometry points vertices じおめとり パターン ジオメトリ 点ごと", Action::Pattern(PatternKind::Geometry)),
        cmd("Mirror X", "mirror symmetry live x みらー さゆう ミラー 左右にうつす", Action::MirrorAxis(0)),
        cmd("Mirror Y", "mirror symmetry live y みらー ぜんご ミラー 前後にうつす", Action::MirrorAxis(1)),
        cmd("Mirror Z", "mirror symmetry live z みらー じょうげ ミラー 上下にうつす", Action::MirrorAxis(2)),
        cmd("Clear Pattern", "pattern remove clear off やめる パターンをやめる", Action::ClearPattern),
        cmd("Make Instance", "instance linked copy いんすたんす りんく インスタンス リンクコピー", Action::MakeInstance),
        cmd("New Scene", "new あたらしく 新しく作る", Action::NewScene),
        cmd("Open", "open load ひらく 開く", Action::Open),
        cmd("Save", "save ほぞん 保存", Action::Save),
        cmd("Save As", "save as なまえ 名前をつけて保存", Action::SaveAs),
        cmd("Export STL", "export stl print 3dぷりんと かきだし STLで書き出す", Action::Export(ExportFormat::Stl)),
        cmd("Export 3MF", "export 3mf slicer すらいさー 3MFで書き出す", Action::Export(ExportFormat::ThreeMf)),
        cmd("Export OBJ", "export obj OBJで書き出す", Action::Export(ExportFormat::Obj)),
        cmd("Export PNG", "export png image screenshot がぞう 画像で書き出す", Action::ExportPng),
        cmd("Print Check", "print check volume size 体積 おおきさ プリントチェック", Action::PrintCheck),
        cmd("Import SVG", "svg import いんぽーと SVGを読み込む", Action::ImportSvg),
        cmd("Import Mesh", "mesh stl obj 3mf ply glb import めっしゅ よみこむ とりこむ 取り込み メッシュを読み込む", Action::ImportMesh),
        cmd("Frame All", "frame all home ぜんたい 全体を見る", Action::FrameAll),
        cmd("Frame Selection", "frame focus selection みる えらんだものを見る", Action::FrameSelection),
        cmd("Toggle Perspective / Ortho", "perspective orthographic ortho えんきん へいこう 遠近 平行を切りかえ", Action::TogglePerspective),
        cmd("Toggle Add / Subtract", "flip boolean subtract add tab 足す 削る を切りかえ", Action::FlipOp),
        cmd("Next: Add", "add union つぎは足す", Action::SetOp(Op::Add)),
        cmd("Next: Subtract", "subtract difference cut けずる つぎは削る", Action::SetOp(Op::Subtract)),
        cmd("Next: Paint", "paint stain ぬる つぎはぬる", Action::SetOp(Op::Paint)),
        cmd("Next: Intersect", "intersect かさねる つぎは重ねる", Action::SetOp(Op::Intersect)),
        cmd("Tool: Select", "tool select q えらぶ 道具 選ぶ", Action::SetTool(Tool::Select)),
        cmd("Tool: All", "tool all gizmo a ぜんぶ 道具 ぜんぶ", Action::SetTool(Tool::All)),
        cmd("Tool: Move", "tool move translate w うごかす 道具 動かす", Action::SetTool(Tool::Move)),
        cmd("Tool: Rotate", "tool rotate e まわす 道具 回す", Action::SetTool(Tool::Rotate)),
        cmd("Tool: Scale", "tool scale r おおきさ 道具 大きさ", Action::SetTool(Tool::Scale)),
        cmd("View Front", "view front 1 しょうめん 正面から見る", Action::ViewFront),
        cmd("View Right", "view right 3 みぎ 右から見る", Action::ViewRight),
        cmd("View Top", "view top 7 うえ 真上から見る", Action::ViewTop),
        cmd("Show Library", "library assets らいぶらり あせっと ライブラリを開く", Action::ShowAssets),
        cmd("Show Outliner", "outliner scene しーん あうとらいなー シーン一覧を開く", Action::ShowOutliner),
        cmd("Keyboard Shortcuts", "help keys keyboard へるぷ ? 操作のしかた", Action::Help),
    ]);
    v
}

/// How well `q` matches an entry: None = no match, lower is better.
fn score(e: &Entry, q: &str) -> Option<u32> {
    let label = e.label.to_lowercase();
    if label == q {
        return Some(0);
    }
    if label.starts_with(q) {
        return Some(1);
    }
    if label.contains(q) {
        return Some(2);
    }
    if e.words.split_whitespace().any(|w| w == q) {
        return Some(3);
    }
    if e.words.split_whitespace().any(|w| w.starts_with(q)) {
        return Some(4);
    }
    if e.words.contains(q) {
        return Some(5);
    }
    // letters in order ("ssp" finds "sphere ...")
    let mut it = label.chars().chain(std::iter::once(' ')).chain(e.words.chars());
    if q.chars().all(|c| it.any(|x| x == c)) {
        return Some(6);
    }
    None
}

/// The entries that match, best first, at most `limit`. An empty query lists the commands people use most.
pub fn search(all: &[Entry], query: &str, limit: usize) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return all.iter().enumerate().filter(|(_, e)| e.tag != "Library").map(|(i, _)| i).take(limit).collect();
    }
    let mut hits: Vec<(u32, usize)> = all.iter().enumerate().filter_map(|(i, e)| score(e, &q).map(|s| (s, i))).collect();
    hits.sort_by_key(|(s, i)| (*s, *i));
    hits.into_iter().map(|(_, i)| i).take(limit).collect()
}

#[derive(Clone, Default)]
struct State {
    query: String,
    sel: usize,
    fresh: bool,
}

fn id() -> egui::Id {
    egui::Id::new("palette_state")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), State { fresh: true, ..State::default() }));
}

#[allow(dead_code)]
#[allow(dead_code)]
pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<State>(id())).is_some()
}

impl App {
    /// Do what a palette entry says.
    pub fn run_action(&mut self, ctx: &egui::Context, a: Action) {
        let pi = std::f32::consts::PI;
        match a {
            Action::AddShape(k) => self.add_shape(ctx, k),
            Action::AddGroup => self.add_group(),
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

    /// Draw the palette when it is open and run the chosen entry.
    pub fn palette_ui(&mut self, ctx: &egui::Context) {
        let Some(mut st) = ctx.data(|m| m.get_temp::<State>(id())) else { return };
        let all = ENTRIES.get_or_init(entries);
        let hits = search(all, &st.query, 9);
        st.sel = st.sel.min(hits.len().saturating_sub(1));
        let (mut close, mut run) = (false, None);
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                close = true;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
                st.sel = (st.sel + 1).min(hits.len().saturating_sub(1));
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
                st.sel = st.sel.saturating_sub(1);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Enter) {
                run = hits.get(st.sel).copied();
            }
        });
        let before = st.query.clone();
        let rect = self.view_rect;
        egui::Area::new(egui::Id::new("palette_area"))
            .order(egui::Order::Tooltip)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(rect.center().x, rect.top() + 70.0))
            .show(ctx, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_width(460.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.query)
                            .hint_text("Search shapes, library, commands (Japanese also works)")
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::proportional(15.0)),
                    );
                    if st.fresh {
                        r.request_focus();
                        st.fresh = false;
                    }
                    ui.add_space(4.0);
                    if hits.is_empty() {
                        ui.label(RichText::new("No results").color(style::DIM));
                    }
                    for (n, &i) in hits.iter().enumerate() {
                        let Some(e) = all.get(i) else { continue };
                        let (rc, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), egui::Sense::click());
                        if n == st.sel {
                            ui.painter().rect_filled(rc, 6.0, Color32::from_rgba_unmultiplied(52, 178, 120, 80));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rc, 6.0, Color32::from_white_alpha(16));
                        }
                        ui.painter().text(
                            rc.left_center() + Vec2::new(10.0, 0.0),
                            Align2::LEFT_CENTER,
                            &e.label,
                            egui::FontId::proportional(13.5),
                            style::TEXT,
                        );
                        ui.painter().text(
                            rc.right_center() - Vec2::new(10.0, 0.0),
                            Align2::RIGHT_CENTER,
                            e.tag,
                            egui::FontId::proportional(11.5),
                            style::DIM,
                        );
                        if resp.hovered() && ui.input(|i| i.pointer.delta().length() > 0.5) {
                            st.sel = n;
                        }
                        if resp.clicked() {
                            run = Some(i);
                        }
                    }
                });
            });
        if st.query != before {
            st.sel = 0;
        }
        if let Some(i) = run
            && let Some(e) = all.get(i)
        {
            let a = e.action;
            close = true;
            self.run_action(ctx, a);
        }
        if close {
            ctx.data_mut(|m| m.remove::<State>(id()));
        } else {
            ctx.data_mut(|m| m.insert_temp(id(), st));
        }
    }
}

static ENTRIES: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    fn top(q: &str) -> Option<Action> {
        let all = entries();
        search(&all, q, 5).first().and_then(|i| all.get(*i)).map(|e| e.action)
    }

    #[test]
    fn japanese_and_english_names_find_the_same_thing() {
        assert_eq!(top("sphere"), Some(Action::AddShape(Kind::Sphere)));
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
        let all = entries();
        assert!(all.iter().any(|e| matches!(e.action, Action::Insert(_, _))), "the library is in the palette");
        let first = library::all().first().and_then(|c| c.items.first()).map(|i| i.name.clone()).unwrap_or_default();
        let hits = search(&all, &first, 5);
        assert!(hits.iter().any(|i| all.get(*i).is_some_and(|e| e.label == first)));
    }

    #[test]
    fn empty_and_nonsense_queries_are_safe() {
        let all = entries();
        let n = search(&all, "", 9).len();
        assert!(n > 0 && n <= 9);
        assert!(search(&all, "zzzzqqqqxxxx😀", 9).is_empty());
        assert!(search(&all, "   ", 3).len() <= 3);
    }

    #[test]
    fn best_matches_come_first() {
        let all = entries();
        let hits = search(&all, "コピー", 20);
        let labels: Vec<&str> = hits.iter().filter_map(|i| all.get(*i)).map(|e| e.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"Copy"), "{labels:?}");
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
            if matches!(
                e.action,
                Action::Open | Action::Save | Action::SaveAs | Action::Export(_) | Action::ExportPng | Action::ImportSvg | Action::ImportMesh
            ) {
                continue;
            }
            let mut a = App::new(true, None);
            a.run_action(&ctx, e.action);
        }
    }
}
