//! The right side: tabs for the selected thing's shape and material, the melt, and the room;
//! plus the scene list shown on the left.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, Tab};
use crate::materials::{self, Category};
use crate::model::{BlendMode, Corner, Kind, Light, MAX_LIGHTS, MAX_OUTLINE, MAX_SAVED, Material, Melt, Op, Pick, ROUND_MAX};
use crate::sdf;
use crate::deform::{Axis, Deformation, Twist};
use crate::view::targets;
use crate::{glyphs, style};

/// (label, mode, amount, drip, ripple)
const MELT_PRESETS: [(&str, BlendMode, f32, f32, f32); 6] = [
    ("かたい", BlendMode::Smooth, 0.0, 0.0, 1.0),
    ("やわらか", BlendMode::Smooth, 1.0, 0.0, 1.0),
    ("ぷりぷり", BlendMode::Round, 1.6, 0.0, 1.0),
    ("チーズ", BlendMode::Gooey, 1.0, 8.0, 1.0),
    ("はちみつ", BlendMode::Gooey, 1.5, 16.0, 0.7),
    ("とろとろ", BlendMode::Gooey, 2.4, 28.0, 1.0),
];

/// A non-destructive Twist preset applied to the selected existing primitive.
/// The source kind, outline and imported mesh remain editable underneath it.
fn twist_section(ui: &mut Ui, shape: &mut crate::model::Shape) {
    ui.add_space(6.0);
    ui.separator();
    style::title(ui, "Twist（ひねり）");
    ui.label(RichText::new("元の形を残したまま、断面を滑らかに回転").color(style::DIM).size(12.0));
    let mut twist = match shape.deformation {
        Some(Deformation::Twist(t)) => Some(t),
        None => None,
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
        for degrees in [90.0_f32, 180.0, 360.0] {
            let chosen = twist.is_some_and(|t| (t.degrees - degrees).abs() < 0.01);
            if style::pill_enabled(ui, &format!("{degrees:.0}°"), chosen, true).clicked() {
                let mut t = twist.unwrap_or_default();
                t.degrees = degrees;
                twist = Some(t);
            }
        }
        if style::pill_enabled(ui, "解除", twist.is_none(), true).clicked() {
            twist = None;
        }
    });
    if let Some(mut t) = twist {
        ui.horizontal(|ui| {
            ui.label(RichText::new("中心軸").color(style::DIM).size(12.0));
            for axis in Axis::ALL {
                ui.selectable_value(&mut t.axis, axis, axis.label());
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("方向").color(style::DIM).size(12.0));
            ui.selectable_value(&mut t.reverse, false, "＋方向");
            ui.selectable_value(&mut t.reverse, true, "逆方向");
        });
        style::labeled_slider(ui, "角度", &mut t.degrees, 0.0..=720.0, "元の形の軸に沿った回転量 (°)");
        style::labeled_slider(ui, "開始位置", &mut t.begin, 0.0..=0.98, "元形状の軸の下端=0、上端=1");
        t.end = t.end.max(t.begin + 0.02);
        style::labeled_slider(ui, "終了位置", &mut t.end, (t.begin + 0.02)..=1.0, "この位置でねじり角度の変化が終了");
        style::labeled_slider(ui, "下側の量", &mut t.lower, 0.0..=2.0, "角度に対する下側の倍率 (0 は回転なし)");
        style::labeled_slider(ui, "上側の量", &mut t.upper, 0.0..=2.0, "角度に対する上側の倍率 (1 は指定角度)");
        t.sanitize();
        if shape.kind == Kind::Cylinder && t.axis == Axis::Z && (shape.scale[0] - shape.scale[1]).abs() < 0.01 {
            ui.label(RichText::new("真円柱を中心軸でねじっても外形は同じです。角柱や箱で効果を確認できます。").color(style::DIM).size(11.0));
        }
        ui.label(RichText::new("ドラッグ中は軽量表示、停止後は高品質表示。保存や3D書き出しは元データから計算します。").color(style::DIM).size(11.0));
        shape.deformation = Some(Deformation::Twist(t));
    } else {
        shape.deformation = None;
    }
}

const BACKDROPS: [[f32; 3]; 10] = [
    [0.27, 0.27, 0.285],
    [0.12, 0.12, 0.14],
    [0.93, 0.93, 0.94],
    [0.62, 0.78, 0.95],
    [0.99, 0.80, 0.88],
    [0.78, 0.92, 0.82],
    [0.99, 0.90, 0.62],
    [0.30, 0.20, 0.34],
    [0.86, 0.45, 0.35],
    [0.18, 0.30, 0.50],
];

impl App {
    pub fn side(&mut self, ui: &mut Ui) {
        egui::Panel::right("side")
            .exact_size(312.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(14, 12)))
            .show(ui, |ui| {
                style::panel_sheen(ui, ui.max_rect().expand2(vec2(14.0, 12.0)), style::PANEL, style::Edge::Left);
                let tabs = [(Tab::Shape, "形"), (Tab::Material, "ざいしつ"), (Tab::Melt, "メルト"), (Tab::Room, "ライト・背景")];
                let items: Vec<(&str, &str)> = tabs.iter().map(|t| (t.1, "")).collect();
                let sel = tabs.iter().position(|t| t.0 == self.tab);
                if let Some(i) = style::segmented(ui, "side_tabs", &items, sel, true)
                    && let Some(t) = tabs.get(i)
                {
                    self.tab = t.0;
                }
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Shape => self.inspector(ui),
                    Tab::Material => self.material_tab(ui),
                    Tab::Melt => self.melt_panel(ui),
                    Tab::Room => self.room_panel(ui),
                });
            });
    }

    /// The menu on a right click in the viewport.
    pub fn context_menu(&mut self, ui: &mut Ui) {
        if self.sel.is_none() {
            ui.label(RichText::new("何も選ばれていません").color(style::DIM));
            return;
        }
        if ui.button("複製　Ctrl+D").clicked() {
            self.duplicate();
            ui.close();
        }
        if ui.button("＋ ⇄ −　Tab").clicked() {
            self.flip_op();
            ui.close();
        }
        if ui.button("ここを見る　F").clicked() {
            self.frame_view(true);
            ui.close();
        }
        if ui.button("名前を変える").clicked() {
            self.renaming = self.sel;
            ui.close();
        }
        ui.separator();
        if ui.button(RichText::new("削除　Delete").color(style::CUT)).clicked() {
            self.delete_selected();
            ui.close();
        }
    }

    // ----- layers -----

    pub fn layers(&mut self, ui: &mut Ui) {
        self.row_rects.clear();
        ui.spacing_mut().item_spacing.y = 2.0;
        let ngroups = self.scene.groups.len();
        // "x12" after a patterned group, "link" after a linked copy (looked up once for the whole list)
        let badges: Vec<String> = if self.scene.groups.iter().any(|g| g.pattern.is_some() || g.is_linked()) {
            let resolved = crate::pattern::resolve(&self.scene);
            (0..ngroups)
                .map(|gi| {
                    let g = &self.scene.groups[gi];
                    let n = resolved.iter().find(|r| r.gi == gi).map_or(0, |r| r.insts.len().max(1));
                    match (g.is_linked(), g.pattern.is_some()) {
                        (true, true) => format!("リンク ×{n}"),
                        (true, false) => "リンク".to_string(),
                        (false, true) => format!("×{n}"),
                        _ => String::new(),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        for gi in 0..ngroups {
            let folded = self.folded.contains(&gi);
            let sel = self.sel == Some(Pick::Group(gi));
            let (name, op, visible, nshapes) = {
                let g = &self.scene.groups[gi];
                (g.name.clone(), g.op, g.visible, g.shapes.len())
            };
            let id = ui.id().with(("g", gi));
            let badge = badges.get(gi).cloned().unwrap_or_default();
            let row = self.row(ui, id, sel, 0.0, name, Pick::Group(gi), op, visible, glyphs::group, !folded, true, &badge);
            if row.chevron {
                if folded {
                    self.folded.remove(&gi);
                } else {
                    self.folded.insert(gi);
                }
            }
            if row.select {
                self.sel = Some(Pick::Group(gi));
            }
            if row.drag_started {
                self.layer_drag = Some(Pick::Group(gi));
            }
            if row.eye {
                self.scene.groups[gi].visible = !visible;
            }
            if row.op {
                self.scene.groups[gi].op = op.next();
            }
            if !folded {
                for si in 0..nshapes {
                    let Some(s) = self.scene.shape(gi, si) else { continue };
                    let (name, op, visible, material) = (s.name.clone(), s.op, s.visible, s.material);
                    let sel = self.sel == Some(Pick::Shape(gi, si));
                    let id = ui.id().with(("s", gi, si));
                    let row = self.row(
                        ui,
                        id,
                        sel,
                        18.0,
                        name,
                        Pick::Shape(gi, si),
                        op,
                        visible,
                        move |p, r, _| materials::swatch(p, r.center(), 8.5, &material),
                        false,
                        false,
                        "",
                    );
                    if row.select {
                        self.sel = Some(Pick::Shape(gi, si));
                    }
                    if row.drag_started {
                        self.layer_drag = Some(Pick::Shape(gi, si));
                    }
                    if row.eye
                        && let Some(s) = self.scene.shape_mut(gi, si)
                    {
                        s.visible = !visible;
                    }
                    if row.op
                        && let Some(s) = self.scene.shape_mut(gi, si)
                    {
                        s.op = op.next();
                    }
                }
            }
        }
        if ngroups == 0 {
            ui.label(RichText::new("上の形をクリックして足そう").color(style::DIM).size(12.0));
        }
        self.layer_drop(ui);
    }

    /// While a row is being dragged: show where it would land, and put it there on release.
    fn layer_drop(&mut self, ui: &mut Ui) {
        let Some(from) = self.layer_drag else { return };
        let (pos, down) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        let target =
            pos.and_then(|p| self.row_rects.iter().find(|(_, r)| r.y_range().contains(p.y) && r.x_range().contains(p.x)).map(|(k, r)| (*k, *r, p)));
        if let Some((to, rect, p)) = target {
            let after = p.y > rect.center().y;
            let y = if after { rect.bottom() } else { rect.top() };
            let line = [egui::pos2(rect.left() + 4.0, y), egui::pos2(rect.right() - 4.0, y)];
            ui.painter().line_segment(line, Stroke::new(5.0, style::ACCENT.gamma_multiply(0.25)));
            ui.painter().line_segment(line, Stroke::new(2.0, style::ACCENT));
            if !down {
                self.move_layer(from, to, after);
                self.layer_drag = None;
            }
        }
        if !down {
            self.layer_drag = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut Ui,
        id: egui::Id,
        selected: bool,
        indent: f32,
        name: String,
        pick: Pick,
        op: Op,
        visible: bool,
        icon: impl FnOnce(&egui::Painter, Rect, Color32),
        chevron_open: bool,
        has_chevron: bool,
        badge: &str,
    ) -> RowResult {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click_and_drag());
        self.row_rects.push((pick, rect));
        let mut out = RowResult::default();
        style::row_bg(ui, &resp, rect, selected);
        // the grip says "drag me"
        let grip = Color32::from_white_alpha(if resp.hovered() || selected { 120 } else { 44 });
        style::gfx::grip(ui.painter(), egui::pos2(rect.left() + 11.0, rect.center().y), grip);
        let mut x = rect.left() + 18.0 + indent;
        if has_chevron {
            let cr = Rect::from_center_size(egui::pos2(x + 6.0, rect.center().y), vec2(16.0, 20.0));
            let r = ui.interact(cr, id.with("c"), Sense::click());
            let c = cr.center();
            let pts = if chevron_open {
                vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
            } else {
                vec![c + vec2(-2.0, -4.0), c + vec2(-2.0, 4.0), c + vec2(3.0, 0.0)]
            };
            ui.painter().add(egui::Shape::convex_polygon(pts, if r.hovered() { Color32::WHITE } else { style::DIM }, Stroke::NONE));
            out.chevron = r.clicked();
            x += 16.0;
        }
        let ir = Rect::from_min_size(egui::pos2(x, rect.center().y - 10.0), vec2(20.0, 20.0));
        icon(ui.painter(), ir, style::TEXT);
        x += 26.0;
        let eye = Rect::from_center_size(egui::pos2(rect.right() - 18.0, rect.center().y), vec2(22.0, 22.0));
        let opr = Rect::from_center_size(egui::pos2(rect.right() - 44.0, rect.center().y), vec2(22.0, 22.0));
        let name_rect = Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(opr.left() - 4.0, rect.bottom()));
        if self.renaming == Some(pick) {
            let mut text = name.clone();
            let r = ui.put(name_rect.shrink2(vec2(0.0, 3.0)), egui::TextEdit::singleline(&mut text).font(FontId::proportional(13.0)));
            r.request_focus();
            match pick {
                Pick::Group(g) => {
                    if let Some(gr) = self.scene.groups.get_mut(g) {
                        gr.name = text;
                    }
                }
                Pick::Shape(g, s) => {
                    if let Some(sh) = self.scene.shape_mut(g, s) {
                        sh.name = text;
                    }
                }
            }
            if r.lost_focus() {
                self.renaming = None;
            }
        } else {
            let col = if visible { style::TEXT } else { style::DIM };
            let clipped = ui.painter().with_clip_rect(name_rect);
            let used = clipped.text(egui::pos2(name_rect.left(), rect.center().y), Align2::LEFT_CENTER, &name, FontId::proportional(13.0), col);
            if !badge.is_empty() {
                clipped.text(egui::pos2(used.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, badge, FontId::proportional(11.5), style::ACCENT);
            }
        }
        let er = ui.interact(eye, id.with("e"), Sense::click());
        let orr = ui.interact(opr, id.with("o"), Sense::click());
        for r in [&er, &orr] {
            if r.hovered() {
                ui.painter().rect_filled(r.rect, 7.0, Color32::from_white_alpha(22));
            }
        }
        glyphs::eye(ui.painter(), eye, visible, if er.hovered() { Color32::WHITE } else { style::DIM });
        let opcol = style::op_color(op);
        glyphs::op(ui.painter(), opr, op, opcol.gamma_multiply(if orr.hovered() { 1.0 } else { 0.8 }));
        out.eye = er.clicked();
        out.op = orr.clicked();
        out.select = resp.clicked() && !out.eye && !out.op;
        out.drag_started = resp.drag_started() && !out.eye && !out.op;
        if resp.double_clicked() && !out.eye && !out.op {
            self.renaming = Some(pick);
        }
        out
    }

    // ----- inspector -----

    fn inspector(&mut self, ui: &mut Ui) {
        match self.sel {
            Some(Pick::Shape(g, s)) if self.scene.shape(g, s).is_some() => self.shape_inspector(ui, g, s),
            Some(Pick::Group(g)) if g < self.scene.groups.len() => self.group_inspector(ui, g),
            _ => {
                style::title(ui, "えらんだもの");
                ui.label(RichText::new("画面の形か、左のシーンをクリック").color(style::DIM).size(12.0));
            }
        }
    }

    fn actions(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if style::icon_button(ui, 34.0, "複製 (Ctrl+D)", false, glyphs::dup).clicked() {
                self.duplicate();
            }
            if style::icon_button(ui, 34.0, "削除 (Delete)", false, glyphs::trash).clicked() {
                self.delete_selected();
            }
            if matches!(self.sel, Some(Pick::Shape(..)))
                && style::pill(ui, "左右コピー", false).on_hover_text("反対がわにコピー (耳・目・うでに)").clicked()
            {
                self.mirror_copy();
            }
        });
    }

    fn op_pills(ui: &mut Ui, op: &mut Op) {
        let ops = [
            (Op::Add, "＋ 足す", "形をくっつける"),
            (Op::Subtract, "− 削る", "くり抜く"),
            (Op::Paint, "● ぬる", "形は変えずに、色・ざいしつだけぬる"),
            (Op::Intersect, "◐ 重ねる", "前の形と かさなる ところだけ のこす"),
        ];
        let items: Vec<(&str, &str)> = ops.iter().map(|o| (o.1, o.2)).collect();
        let sel = ops.iter().position(|o| o.0 == *op);
        if let Some(i) = style::segmented(ui, "op_pills", &items, sel, true)
            && let Some(o) = ops.get(i)
        {
            *op = o.0;
        }
    }

    fn group_inspector(&mut self, ui: &mut Ui, g: usize) {
        style::title(ui, "グループ");
        let n = self.scene.groups.len();
        let mut swap = None;
        {
            let gr = &mut self.scene.groups[g];
            ui.text_edit_singleline(&mut gr.name);
            Self::op_pills(ui, &mut gr.op);
            style::labeled_slider(
                ui,
                "前の組と\nとけあう",
                &mut gr.blend,
                0.0..=80.0,
                "このグループ全体が、前のグループとどのくらい溶けるか (グープ)",
            );
        }
        ui.horizontal(|ui| {
            if style::pill_enabled(ui, "↑ 前へ", false, g > 0).clicked() {
                swap = Some((g, g - 1));
            }
            if style::pill_enabled(ui, "↓ 後ろへ", false, g + 1 < n).clicked() {
                swap = Some((g, g + 1));
            }
        });
        if let Some((a, b)) = swap {
            self.scene.groups.swap(a, b);
            self.sel = Some(Pick::Group(b));
        }
        self.pattern_section(ui, g);
        self.actions(ui);
        let linked = self.scene.groups.get(g).is_some_and(crate::model::Group::is_linked);
        let hint = if linked { "リンクコピーの位置・回転・大きさは、上のらんで変えます" } else { "グループごと動かす・回す・大きさを変えられます" };
        ui.label(RichText::new(hint).size(12.0).color(style::DIM));
    }

    fn shape_inspector(&mut self, ui: &mut Ui, g: usize, s: usize) {
        style::title(ui, "形");
        let ngroups = self.scene.groups.len();
        let nshapes = self.scene.groups[g].shapes.len();
        let mut move_to: Option<usize> = None;
        let mut reorder: Option<usize> = None;
        let mut outline_changed = false;
        let mut top_changed = false;
        {
            let Some(sh) = self.scene.shape_mut(g, s) else { return };
            ui.text_edit_singleline(&mut sh.name);
            Self::op_pills(ui, &mut sh.op);
            if sh.kind == Kind::Mesh {
                // an imported mesh cannot become a primitive by a click: it shows where it came from instead
                crate::mesh_dialog::inspector(ui, sh);
            } else {
                // change what kind of shape this is, keeping where it sits
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("入れかえ").color(style::DIM).size(12.0));
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for k in Kind::ALL {
                        let on = sh.kind == k;
                        if style::icon_button(ui, 32.0, k.label(), on, |p, r, ink| glyphs::kind(p, r, k, ink, Color32::from_white_alpha(18))).clicked()
                            && !on
                        {
                            if sh.name == sh.kind.label() {
                                sh.name = k.label().to_string();
                            }
                            sh.kind = k;
                            sh.size = k.default_size();
                            sh.params = k.default_params();
                            if k.uses_path() && sh.path.len() < 2 {
                                sh.path = k.default_path();
                            }
                            if k.uses_outline2() && sh.outline2.len() < 3 {
                                sh.outline2 = k.default_outline2();
                            }
                            if matches!(k, Kind::Revolve | Kind::Sweep) {
                                sh.outline = k.default_outline();
                            }
                        }
                    }
                });
            }
            style::xyz(ui, "位置", &mut sh.pos, 0.5, -5000.0..=5000.0, "");
            style::xyz(ui, "回転", &mut sh.rot, 1.0, -3600.0..=3600.0, "°");
            style::xyz(ui, "大きさ", &mut sh.scale, 0.01, 0.01..=100.0, "×");
            let labels = sh.kind.size_labels();
            ui.horizontal_wrapped(|ui| {
                for (i, l) in labels.iter().enumerate() {
                    if !l.is_empty() {
                        ui.label(RichText::new(*l).color(style::DIM).size(12.0));
                        style::field(ui, egui::DragValue::new(&mut sh.size[i]).speed(0.3).range(0.1..=2000.0).suffix(" mm").max_decimals(1));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (i, l) in sh.kind.param_labels().iter().enumerate() {
                    if !l.is_empty() {
                        ui.label(RichText::new(*l).color(style::DIM).size(12.0));
                        style::field(ui, egui::DragValue::new(&mut sh.params[i]).speed(0.1).max_decimals(2));
                    }
                }
            });
            if sh.kind.can_round() {
                style::labeled_slider(ui, "丸み", &mut sh.round, 0.0..=ROUND_MAX, "角やふちの丸さ (mm)。大きすぎても、形がこわれない所でとまります");
                let styles = [("丸", "角を丸くする (フィレット)"), ("面取り", "角をななめに平らに切り落とす (チャンファー)")];
                let now = Corner::ALL.iter().position(|c| *c == sh.corner);
                if let Some(i) = style::segmented(ui, "corner_style", &styles, now, true) {
                    sh.corner = Corner::ALL.get(i).copied().unwrap_or_default();
                }
            }
            style::labeled_slider(ui, "とけあい", &mut sh.blend, 0.0..=80.0, "この形が、まわりとどのくらい溶けてくっつくか (グープ)");
            style::labeled_slider(ui, "ちゅうぐう\nシェル", &mut sh.shell, 0.0..=40.0, "中をくり抜いて、かべの あつさ(mm)だけ のこす。0 = つまっている");
            twist_section(ui, sh);
            if sh.kind.uses_outline() {
                outline_changed = true;
                top_changed = sh.kind.uses_outline2();
            }
        }
        if outline_changed {
            self.sketch_edit_button(ui, g, s);
            self.sketch_editor(ui, g, s);
        }
        if top_changed {
            self.sketch_editor_top(ui, g, s);
        }
        if self.scene.shape(g, s).is_some_and(|x| x.kind.uses_path()) {
            self.curve_inspector(ui, g, s);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if style::pill_enabled(ui, "↑", false, s > 0).on_hover_text("ひとつ前へ").clicked() {
                reorder = Some(s - 1);
            }
            if style::pill_enabled(ui, "↓", false, s + 1 < nshapes).on_hover_text("ひとつ後ろへ").clicked() {
                reorder = Some(s + 1);
            }
            if ngroups > 1 {
                egui::ComboBox::from_id_salt("move_to").selected_text("グループを移す").show_ui(ui, |ui| {
                    for (i, gr) in self.scene.groups.iter().enumerate() {
                        if i != g && ui.selectable_label(false, &gr.name).clicked() {
                            move_to = Some(i);
                        }
                    }
                });
            }
        });
        if let Some(t) = reorder {
            self.scene.groups[g].shapes.swap(s, t);
            self.sel = Some(Pick::Shape(g, t));
        }
        if let Some(t) = move_to
            && self.scene.groups[g].shapes.len() > s
        {
            let sh = self.scene.groups[g].shapes.remove(s);
            self.scene.groups[t].shapes.push(sh);
            self.sel = Some(Pick::Shape(t, self.scene.groups[t].shapes.len() - 1));
        }
        self.actions(ui);
    }

    // ----- materials -----

    fn material_tab(&mut self, ui: &mut Ui) {
        let mut tg = targets(&self.scene, self.sel);
        if self.mat_group
            && let Some(Pick::Shape(g, _)) = self.sel
            && let Some(gr) = self.scene.groups.get(g)
        {
            tg = (0..gr.shapes.len()).map(|s| (g, s)).collect();
        }
        let Some(&(g0, s0)) = tg.first() else {
            style::title(ui, "ざいしつ");
            ui.label(RichText::new("形を選ぶと、ここでざいしつを選べます").color(style::DIM).size(12.0));
            return;
        };
        let Some(first) = self.scene.shape(g0, s0).map(|s| s.material) else { return };
        let mut m = first;
        let presets = materials::presets();
        let current = presets.iter().find(|p| p.material == m).map(|p| p.name);

        // the big ball, the name, and "keep"
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
            style::gfx::vgrad(ui.painter(), rect, 14.0, style::INSET_TOP, style::INSET_BOTTOM);
            ui.painter().rect_stroke(rect, 14.0, Stroke::new(1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
            style::gfx::shadow(&ui.painter_at(rect), Rect::from_center_size(rect.center() + vec2(0.0, 24.0), vec2(36.0, 4.0)), 2.0, &[(0, 10, 0, 90)]);
            materials::swatch(&ui.painter_at(rect), rect.center(), 28.0, &m);
            ui.vertical(|ui| {
                ui.label(RichText::new(current.unwrap_or("オリジナル")).strong().size(14.0));
                ui.horizontal(|ui| {
                    let save_reason = if self.scene.saved.contains(&m) {
                        Some("この材質は、すでに「★マイ」に保存されています")
                    } else if self.scene.saved.len() >= MAX_SAVED {
                        Some("保存できる数の上限です。右クリックで保存した材質を消してください")
                    } else {
                        None
                    };
                    let save = style::pill_enabled(ui, "★ ほぞん", false, save_reason.is_none());
                    if let Some(reason) = save_reason {
                        save.on_hover_text(reason);
                    } else if save.clicked() {
                        self.scene.saved.push(m);
                        self.mat_cat = None;
                    }
                });
                if matches!(self.sel, Some(Pick::Shape(..))) {
                    style::checkbox(ui, &mut self.mat_group, RichText::new("グループ全部に").size(12.5));
                }
            });
        });
        ui.add_space(4.0);

        // categories
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for c in Category::ALL {
                if style::pill(ui, c.label(), self.mat_cat == Some(c)).clicked() {
                    self.mat_cat = Some(c);
                }
            }
            if style::pill(ui, "★マイ", self.mat_cat.is_none()).clicked() {
                self.mat_cat = None;
            }
        });
        ui.add_space(2.0);

        // the balls to pick from
        let mut remove: Option<usize> = None;
        let mut chosen: Option<Material> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            let cell = |ui: &mut Ui, name: &str, mat: &Material, on: bool| -> egui::Response {
                let (rect, resp) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::click());
                let p = ui.painter_at(rect.expand(6.0));
                style::tile(ui, &resp, rect, on);
                materials::swatch(&p, rect.center(), 15.0, mat);
                resp.on_hover_text(name)
            };
            match self.mat_cat {
                Some(cat) => {
                    for pr in presets.iter().filter(|p| p.category == cat) {
                        if cell(ui, pr.name, &pr.material, pr.material == m).clicked() {
                            chosen = Some(pr.material);
                        }
                    }
                }
                None => {
                    if self.scene.saved.is_empty() {
                        ui.label(RichText::new("「★ ほぞん」でここにためられます").size(12.0).color(style::DIM));
                    }
                    for (i, sm) in self.scene.saved.iter().enumerate() {
                        let r = cell(ui, "クリックで使う / 右クリックで消す", sm, *sm == m);
                        if r.clicked() {
                            chosen = Some(*sm);
                        }
                        if r.secondary_clicked() {
                            remove = Some(i);
                        }
                    }
                }
            }
        });
        if let Some(i) = remove {
            self.scene.saved.remove(i);
        }
        if let Some(c) = chosen {
            m = c;
        }

        // fine tuning
        style::title(ui, "くわしく");
        ui.horizontal(|ui| {
            style::row_label(ui, "いろ");
            style::color_edit(ui, &mut m.color);
        });
        let mut gloss = 1.0 - m.roughness;
        if style::labeled_slider(ui, "つや", &mut gloss, 0.0..=1.0, "つるつる ⇄ ざらざら") {
            m.roughness = 1.0 - gloss;
        }
        style::labeled_slider(ui, "金属", &mut m.metallic, 0.0..=1.0, "プラスチック ⇄ 金属");
        style::labeled_slider(ui, "とうめい", &mut m.glass, 0.0..=1.0, "ガラスのように光を曲げて透ける");
        style::labeled_slider(ui, "ひかる", &mut m.glow, 0.0..=4.0, "自分で光る (ランプ・ネオン)");
        style::labeled_slider(ui, "にじいろ", &mut m.iridescence, 0.0..=1.0, "見る角度で虹色に変わる (シャボン玉・タマムシ)");
        style::labeled_slider(ui, "コート", &mut m.coat, 0.0..=1.0, "ピカピカのクリア層 (車のペンキ・キャンディ)");
        style::labeled_slider(ui, "すけ感", &mut m.translucency, 0.0..=1.0, "光がしみこむ (ワックス・ゼリー・こおり)");
        gradient_controls(ui, &mut m);

        if m != first {
            m.sanitize();
            for (g, s) in tg {
                if let Some(sh) = self.scene.shape_mut(g, s) {
                    sh.material = m;
                }
            }
        }
    }

    // ----- the room: lights and backdrop -----

    fn room_panel(&mut self, ui: &mut Ui) {
        style::title(ui, "雰囲気");
        let looks = crate::model::Environment::looks();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (name, env) in looks {
                if style::pill(ui, name, false).clicked() {
                    self.scene.env = env;
                }
            }
        });
        let e = &mut self.scene.env;
        style::title(ui, "背景");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
            for c in BACKDROPS {
                let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                let col = materials::color32(c);
                let on = e.backdrop == c;
                let hot = ui.ctx().animate_value_with_time(resp.id.with("hot"), if resp.hovered() { 1.0 } else { 0.0 }, 0.1);
                let r = 10.5 + 0.8 * hot;
                ui.painter().circle_filled(rect.center() + vec2(0.0, 1.5), r, Color32::from_black_alpha(80));
                style::gfx::ball(
                    ui.painter(),
                    rect.center(),
                    r,
                    vec2(-0.3, -0.38),
                    &[(0.0, style::gfx::lit(col, 1.25)), (0.6, col), (1.0, style::gfx::lit(col, 0.72))],
                );
                ui.painter().circle_stroke(rect.center(), r, Stroke::new(1.0, Color32::from_white_alpha(46)));
                if on {
                    ui.painter().circle_stroke(rect.center(), 13.5, Stroke::new(2.0, style::ACCENT));
                }
                if resp.clicked() {
                    e.backdrop = c;
                }
            }
            style::color_edit(ui, &mut e.backdrop);
        });
        ui.horizontal(|ui| {
            style::switch(ui, &mut e.grid, "グリッド");
            style::switch(ui, &mut e.floor, "ゆか(影)");
        });
        style::labeled_slider(ui, "明るさ", &mut e.exposure, 0.3..=3.0, "写真の明るさ (露出)");
        style::labeled_slider(ui, "影のこさ", &mut e.shadow, 0.0..=1.0, "");
        style::labeled_slider(ui, "まわりの光", &mut e.ambient, 0.0..=2.0, "影の部分の明るさ・空の映りこみ");
        style::title(ui, "空 (金属やガラスに映る)");
        ui.horizontal(|ui| {
            for (label, c) in [("上", &mut e.sky_top), ("地平線", &mut e.sky_horizon), ("下", &mut e.sky_bottom)] {
                ui.label(RichText::new(label).size(12.0).color(style::DIM));
                style::color_edit(ui, c);
            }
        });
        style::title(ui, "ライト");
        let mut remove = None;
        let n = e.lights.len();
        for (i, l) in e.lights.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("ライト {}", i + 1)).size(12.5).strong());
                style::color_edit(ui, &mut l.color);
                if n > 1 && ui.small_button("消す").on_hover_text("このライトを消す").clicked() {
                    remove = Some(i);
                }
            });
            style::labeled_slider(ui, "むき", &mut l.yaw, -180.0..=180.0, "ぐるっと回す (度)");
            style::labeled_slider(ui, "たかさ", &mut l.pitch, 0.0..=89.0, "ひくい ⇄ 真上 (度)");
            style::labeled_slider(ui, "つよさ", &mut l.power, 0.0..=3.0, "");
            if i == 0 {
                ui.label(RichText::new("ライト1だけが影を落とします").size(12.0).color(style::DIM));
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove {
            e.lights.remove(i);
        }
        if e.lights.len() < MAX_LIGHTS && style::pill(ui, "＋ ライトを足す", false).clicked() {
            e.lights.push(Light { yaw: -60.0, pitch: 35.0, power: 0.6, color: [1.0, 1.0, 1.0] });
        }
    }

    // ----- sketch to extrude -----

    fn sketch_editor(&mut self, ui: &mut Ui, g: usize, s: usize) {
        self.sketch_editor_of(ui, g, s, false);
    }

    /// The loft's top profile.
    fn sketch_editor_top(&mut self, ui: &mut Ui, g: usize, s: usize) {
        self.sketch_editor_of(ui, g, s, true);
    }

    /// The outline editor, for the main profile (`top` = false) or a loft's top profile (`outline2`).
    fn sketch_editor_of(&mut self, ui: &mut Ui, g: usize, s: usize, top: bool) {
        let kind = self.scene.shape(g, s).map(|x| x.kind);
        let revolve = kind == Some(Kind::Revolve);
        let loft = kind == Some(Kind::Loft);
        style::title(
            ui,
            match (revolve, loft, top) {
                (true, _, _) => "断面（はんぶん）",
                (_, true, false) => "下の輪かく",
                (_, true, true) => "上の輪かく",
                _ => "スケッチ（輪かく）",
            },
        );
        if revolve {
            ui.label(RichText::new("よこ = ま中のじくからの きょり、たて = 高さ。じくで ぐるっと回します").size(11.0).color(style::DIM));
        }
        let side = ui.available_width().min(240.0);
        let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        let Some(sh) = self.scene.shape_mut(g, s) else { return };
        let ol = if top { &mut sh.outline2 } else { &mut sh.outline };
        let extent = ol.iter().map(|p| p[0].abs().max(p[1].abs())).fold(20.0_f32, f32::max) * 1.25;
        let k = side * 0.5 / extent;
        let to_px = |p: [f32; 2]| Pos2::new(rect.center().x + p[0] * k, rect.center().y - p[1] * k);
        let to_mm =
            |q: Pos2| [((q.x - rect.center().x) / k).round().clamp(-2000.0, 2000.0), ((rect.center().y - q.y) / k).round().clamp(-2000.0, 2000.0)];
        let p = ui.painter();
        style::gfx::vgrad(p, rect, 8.0, style::INSET_TOP, style::INSET_BOTTOM);
        // grid every 10 mm
        let mut v = -(extent / 10.0).floor() * 10.0;
        while v <= extent {
            let a = to_px([v, -extent]);
            let b = to_px([v, extent]);
            let c = to_px([-extent, v]);
            let d = to_px([extent, v]);
            let w = if v == 0.0 { 1.5 } else { 0.6 };
            p.line_segment([a, b], Stroke::new(w, Color32::from_white_alpha(if v == 0.0 { 40 } else { 14 })));
            p.line_segment([c, d], Stroke::new(w, Color32::from_white_alpha(if v == 0.0 { 40 } else { 14 })));
            v += 10.0;
        }
        let pts: Vec<Pos2> = ol.iter().map(|q| to_px(*q)).collect();
        p.add(egui::Shape::convex_polygon(Vec::new(), Color32::TRANSPARENT, Stroke::NONE));
        let mut path = pts.clone();
        if let Some(f) = pts.first() {
            path.push(*f);
        }
        p.add(egui::Shape::line(path, Stroke::new(2.0, style::ACCENT)));
        let mut remove = None;
        let mut moved = None;
        let mut insert = None;
        for (i, pt) in pts.iter().enumerate() {
            let hr = Rect::from_center_size(*pt, vec2(16.0, 16.0));
            let r = ui.interact(hr, ui.id().with(("pt", g, s, i, top)), Sense::click_and_drag());
            let on = r.hovered() || r.dragged();
            ui.painter().circle_filled(*pt, if on { 6.5 } else { 5.0 }, if on { Color32::WHITE } else { style::ACCENT });
            if r.dragged()
                && let Some(m) = r.interact_pointer_pos()
            {
                moved = Some((i, to_mm(m)));
            }
            if r.secondary_clicked() || r.double_clicked() {
                remove = Some(i);
            }
            r.on_hover_text("ドラッグで動かす / ダブルクリックで消す");
        }
        // a click on an edge adds a corner
        let bg = ui.interact(rect, ui.id().with(("sk", g, s, top)), Sense::click());
        if bg.clicked()
            && let Some(m) = bg.interact_pointer_pos()
        {
            let mut best = (f32::MAX, 0usize);
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                let ab = b - a;
                let t = ((m - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                let d = (m - (a + ab * t)).length();
                if d < best.0 {
                    best = (d, i + 1);
                }
            }
            let m = to_mm(m);
            insert = Some((best.1, if revolve { [m[0].max(0.0), m[1]] } else { m }));
        }
        if let Some((i, m)) = moved
            && let Some(p) = ol.get_mut(i)
        {
            *p = if revolve { [m[0].max(0.0), m[1]] } else { m };
        }
        if let Some((i, m)) = insert
            && ol.len() < MAX_OUTLINE
        {
            ol.insert(i, m);
        }
        if let Some(i) = remove
            && ol.len() > 3
        {
            ol.remove(i);
        }
        ui.label(RichText::new(format!("線をクリックで角を足す　{}/{}個", ol.len(), MAX_OUTLINE)).size(12.0).color(style::DIM));
        ui.horizontal_wrapped(|ui| {
            let shapes: [(&str, Vec<[f32; 2]>); 5] = [
                ("四角", vec![[-15.0, -15.0], [15.0, -15.0], [15.0, 15.0], [-15.0, 15.0]]),
                ("三角", vec![[-17.0, -12.0], [17.0, -12.0], [0.0, 18.0]]),
                (
                    "六角",
                    (0..6)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::PI / 3.0;
                            [18.0 * a.cos(), 18.0 * a.sin()]
                        })
                        .collect(),
                ),
                (
                    "星",
                    (0..10)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::PI / 5.0 + std::f32::consts::FRAC_PI_2;
                            let r = if i % 2 == 0 { 20.0 } else { 8.5 };
                            [r * a.cos(), r * a.sin()]
                        })
                        .collect(),
                ),
                (
                    "十字",
                    vec![
                        [-6.0, -18.0],
                        [6.0, -18.0],
                        [6.0, -6.0],
                        [18.0, -6.0],
                        [18.0, 6.0],
                        [6.0, 6.0],
                        [6.0, 18.0],
                        [-6.0, 18.0],
                        [-6.0, 6.0],
                        [-18.0, 6.0],
                        [-18.0, -6.0],
                        [-6.0, -6.0],
                    ],
                ),
            ];
            let shapes: Vec<(&str, Vec<[f32; 2]>)> = if revolve {
                vec![
                    ("つぼ", Kind::Revolve.default_outline()),
                    ("コップ", vec![[0.0, -20.0], [14.0, -20.0], [18.0, 20.0], [16.0, 20.0], [12.0, -17.0], [0.0, -17.0]]),
                    ("こま", vec![[0.0, -22.0], [4.0, -16.0], [20.0, 0.0], [20.0, 6.0], [6.0, 8.0], [5.0, 22.0], [0.0, 22.0]]),
                    ("四角(円柱)", vec![[0.0, -20.0], [15.0, -20.0], [15.0, 20.0], [0.0, 20.0]]),
                ]
            } else {
                shapes.into_iter().collect()
            };
            for (name, pts) in shapes {
                if style::pill(ui, name, false).clicked() {
                    *ol = pts.into_iter().map(|p| [p[0].round(), p[1].round()]).collect();
                }
            }
        });
    }

    // ----- melting -----

    pub fn melt_panel(&mut self, ui: &mut Ui) {
        style::title(ui, "メルト（とけあい）");
        let m: &mut Melt = &mut self.scene.melt;
        ui.horizontal_wrapped(|ui| {
            for (name, mode, amount, drip, ripple) in MELT_PRESETS {
                let on = m.mode == mode && (m.amount - amount).abs() < 1e-3 && (m.drip - drip).abs() < 1e-3 && (m.ripple - ripple).abs() < 1e-3;
                if style::pill(ui, name, on).clicked() {
                    *m = Melt { mode, amount, drip, ripple };
                }
            }
        });
        melt_preview(ui, m);
        let items: Vec<(&str, &str)> = BlendMode::ALL.iter().map(|b| (b.label(), "")).collect();
        let sel = BlendMode::ALL.iter().position(|b| *b == m.mode);
        if let Some(i) = style::segmented(ui, "melt_modes", &items, sel, true)
            && let Some(b) = BlendMode::ALL.get(i)
        {
            m.mode = *b;
        }
        for (label, value, range, tip) in [
            ("とけぐあい", &mut m.amount, 0.0..=4.0, "全部の形のとけあいを一度に強く・弱く"),
            ("たれ", &mut m.drip, 0.0..=40.0, "下にたれるように溶ける"),
            ("たれの細かさ", &mut m.ripple, 0.3..=4.0, "たれの間かく"),
        ] {
            style::labeled_slider(ui, label, value, range, tip);
        }
        ui.label(RichText::new("形ごとの とけあい は「形」の中で決められます").size(12.0).color(style::DIM));
    }
}

#[derive(Default)]
struct RowResult {
    select: bool,
    eye: bool,
    op: bool,
    chevron: bool,
    drag_started: bool,
}

/// Two blobs melting together with the chosen curve: a live picture of what the settings do.
fn melt_preview(ui: &mut Ui, m: &Melt) {
    let w = ui.available_width().min(284.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
    let p = ui.painter_at(rect);
    style::gfx::vgrad(&p, rect, 10.0, style::INSET_TOP, style::INSET_BOTTOM);
    let cell = 2.0_f32;
    let nx = (rect.width() / cell) as i32;
    let ny = (rect.height() / cell) as i32;
    let r = rect.height() * 0.26;
    let sep = r * 1.75;
    let c1 = rect.center() + vec2(-sep * 0.5, -rect.height() * 0.1);
    let c2 = rect.center() + vec2(sep * 0.5, -rect.height() * 0.1);
    let k = 6.0 * m.amount * (r / 20.0);
    for j in 0..ny {
        for i in 0..nx {
            let pt = rect.min + vec2(i as f32 + 0.5, j as f32 + 0.5) * cell;
            // drips sag the lower half
            let sag = if m.drip > 0.0 { (m.drip / 60.0) * r * 0.9 * (0.5 + 0.5 * (pt.x * 0.08 * m.ripple).sin()) } else { 0.0 };
            let q = pt - vec2(0.0, sag * ((pt.y - rect.top()) / rect.height()).powi(2));
            let d1 = (q - c1).length() - r;
            let d2 = (q - c2).length() - r;
            let (d, h) = sdf::smin(d1, d2, k, m.mode);
            if d < 0.5 {
                let depth = (-d / r).clamp(0.0, 1.0);
                let base = Color32::from_rgb(
                    (236.0 + (120.0 - 236.0) * (1.0 - h)) as u8,
                    (140.0 + (150.0 - 140.0) * (1.0 - h)) as u8,
                    (110.0 + (250.0 - 110.0) * (1.0 - h)) as u8,
                );
                let shade = 0.62 + 0.38 * depth;
                let col = Color32::from_rgb((base.r() as f32 * shade) as u8, (base.g() as f32 * shade) as u8, (base.b() as f32 * shade) as u8);
                p.rect_filled(Rect::from_min_size(rect.min + vec2(i as f32, j as f32) * cell, vec2(cell, cell)), 0.0, col);
            }
        }
    }
    p.text(rect.left_bottom() + vec2(8.0, -6.0), Align2::LEFT_BOTTOM, "プレビュー", FontId::proportional(10.0), Color32::from_white_alpha(70));
}

/// The gradient part of the material tab: a switch, the second colour, the direction and the blend.
fn gradient_controls(ui: &mut Ui, m: &mut Material) {
    style::title(ui, "グラデーション");
    let mut on = m.has_gradient();
    if style::switch(ui, &mut on, "グラデーション").on_hover_text("2つの色が、なめらかにかわる").changed() {
        m.gradient = if on { 1.0 } else { 0.0 };
    }
    if !on {
        return;
    }
    ui.horizontal(|ui| {
        style::row_label(ui, "2つ目の色");
        style::color_edit(ui, &mut m.color2);
    });
    let dirs = [("左右", "X: ひだり → みぎ"), ("前後", "Y: まえ → うしろ"), ("上下", "Z: した → うえ")];
    if let Some(i) = style::segmented(ui, "grad_axis", &dirs, Some(usize::from(m.grad_axis.min(2))), true) {
        m.grad_axis = i as u8;
    }
    ui.horizontal(|ui| {
        style::switch(ui, &mut m.grad_flip, "反転").on_hover_text("色の向きをぎゃくにする");
        style::switch(ui, &mut m.grad_local, "形にそう").on_hover_text("入れると、形を回したり のばしたりしても、色が形についてくる");
    });
    style::labeled_slider(ui, "とけぐあい", &mut m.grad_soft, 0.0..=1.0, "0 = くっきり2色 ⇄ 1 = ゆっくり とける");
    style::labeled_slider(ui, "つよさ", &mut m.gradient, 0.05..=1.0, "おわりの色が どのくらい出るか");
    style::labeled_slider(ui, "ここから", &mut m.grad_from, 0.0..=1.0, "形のどこから色がかわりはじめるか (0 = はし)");
    style::labeled_slider(ui, "ここまで", &mut m.grad_to, 0.0..=1.0, "形のどこで色がかわりおわるか (1 = はんたいのはし)");
}


