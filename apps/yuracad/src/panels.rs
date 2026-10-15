//! The right side: layers, the selected thing's settings, materials and melting.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};

use crate::app::App;
use crate::model::{BlendMode, Kind, MAX_OUTLINE, Material, Melt, Op, Pick};
use crate::sdf;
use crate::{glyphs, style};

const SWATCHES: [(&str, [f32; 3], f32, f32); 12] = [
    ("クレイ", [0.93, 0.62, 0.50], 0.85, 0.0),
    ("ストロベリー", [0.96, 0.40, 0.52], 0.28, 0.0),
    ("ミント", [0.55, 0.90, 0.78], 0.45, 0.0),
    ("スカイ", [0.50, 0.75, 0.98], 0.35, 0.0),
    ("ラベンダー", [0.72, 0.64, 0.96], 0.45, 0.0),
    ("レモン", [0.98, 0.90, 0.46], 0.40, 0.0),
    ("ゴールド", [1.00, 0.78, 0.35], 0.25, 1.0),
    ("カッパー", [0.95, 0.55, 0.40], 0.30, 1.0),
    ("クローム", [0.86, 0.88, 0.92], 0.07, 1.0),
    ("パール", [0.96, 0.94, 0.96], 0.18, 0.25),
    ("ミルク", [0.97, 0.96, 0.93], 0.80, 0.0),
    ("すみ", [0.09, 0.09, 0.11], 0.30, 0.0),
];

/// (label, mode, amount, drip, ripple)
const MELT_PRESETS: [(&str, BlendMode, f32, f32, f32); 6] = [
    ("かたい", BlendMode::Smooth, 0.0, 0.0, 1.0),
    ("やわらか", BlendMode::Smooth, 1.0, 0.0, 1.0),
    ("ぷりぷり", BlendMode::Round, 1.6, 0.0, 1.0),
    ("チーズ", BlendMode::Gooey, 1.0, 8.0, 1.0),
    ("はちみつ", BlendMode::Gooey, 1.5, 16.0, 0.7),
    ("とろとろ", BlendMode::Gooey, 2.4, 28.0, 1.0),
];

impl App {
    pub fn side(&mut self, ui: &mut Ui) {
        egui::Panel::right("side")
            .exact_size(308.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(style::PANEL).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.melt_panel(ui);
                    ui.add_space(6.0);
                    ui.separator();
                    self.layers(ui);
                    ui.add_space(6.0);
                    ui.separator();
                    self.inspector(ui);
                    ui.add_space(30.0);
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

    fn layers(&mut self, ui: &mut Ui) {
        style::title(ui, "レイヤー");
        let ngroups = self.scene.groups.len();
        for gi in 0..ngroups {
            let folded = self.folded.contains(&gi);
            let sel = self.sel == Some(Pick::Group(gi));
            let (name, op, visible, nshapes) = {
                let g = &self.scene.groups[gi];
                (g.name.clone(), g.op, g.visible, g.shapes.len())
            };
            let id = ui.id().with(("g", gi));
            let row = self.row(ui, id, sel, 0.0, name, Pick::Group(gi), op, visible, glyphs::group, !folded, true);
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
            if row.eye {
                self.scene.groups[gi].visible = !visible;
            }
            if row.op {
                self.scene.groups[gi].op = op.flip();
            }
            if !folded {
                for si in 0..nshapes {
                    let Some(s) = self.scene.shape(gi, si) else { continue };
                    let (name, op, visible, kind, color) = (s.name.clone(), s.op, s.visible, s.kind, s.material.color);
                    let sel = self.sel == Some(Pick::Shape(gi, si));
                    let id = ui.id().with(("s", gi, si));
                    let fill = Color32::from_rgb((color[0] * 255.0) as u8, (color[1] * 255.0) as u8, (color[2] * 255.0) as u8);
                    let row = self.row(
                        ui,
                        id,
                        sel,
                        18.0,
                        name,
                        Pick::Shape(gi, si),
                        op,
                        visible,
                        move |p, r, ink| glyphs::kind(p, r, kind, ink, fill.gamma_multiply(0.55)),
                        false,
                        false,
                    );
                    if row.select {
                        self.sel = Some(Pick::Shape(gi, si));
                    }
                    if row.eye
                        && let Some(s) = self.scene.shape_mut(gi, si)
                    {
                        s.visible = !visible;
                    }
                    if row.op
                        && let Some(s) = self.scene.shape_mut(gi, si)
                    {
                        s.op = op.flip();
                    }
                }
            }
        }
        if ngroups == 0 {
            ui.label(RichText::new("左の形をクリックして足そう").color(style::DIM).size(12.0));
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
    ) -> RowResult {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        let mut out = RowResult::default();
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 7.0, style::ACCENT_SOFT);
            p.rect_stroke(rect, 7.0, Stroke::new(1.0, style::ACCENT), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            p.rect_filled(rect, 7.0, style::HOVER);
        }
        let mut x = rect.left() + 6.0 + indent;
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
            ui.painter().with_clip_rect(name_rect).text(
                egui::pos2(name_rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                &name,
                FontId::proportional(13.0),
                col,
            );
        }
        let er = ui.interact(eye, id.with("e"), Sense::click());
        let orr = ui.interact(opr, id.with("o"), Sense::click());
        glyphs::eye(ui.painter(), eye, visible, if er.hovered() { Color32::WHITE } else { style::DIM });
        let opcol = if op == Op::Add { style::ADD } else { style::CUT };
        glyphs::op(ui.painter(), opr, op, opcol.gamma_multiply(if orr.hovered() { 1.0 } else { 0.8 }));
        out.eye = er.clicked();
        out.op = orr.clicked();
        out.select = resp.clicked() && !out.eye && !out.op;
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
                ui.label(RichText::new("画面の形か、レイヤーをクリック").color(style::DIM).size(12.0));
            }
        }
    }

    fn actions(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if style::icon_button(ui, 30.0, "複製 (Ctrl+D)", false, glyphs::dup).clicked() {
                self.duplicate();
            }
            if style::icon_button(ui, 30.0, "削除 (Delete)", false, glyphs::trash).clicked() {
                self.delete_selected();
            }
            if matches!(self.sel, Some(Pick::Shape(..)))
                && style::pill(ui, "左右コピー", false).on_hover_text("反対がわにコピー (耳・目・うでに)").clicked()
            {
                self.mirror_copy();
            }
        });
    }

    fn group_inspector(&mut self, ui: &mut Ui, g: usize) {
        style::title(ui, "グループ");
        let n = self.scene.groups.len();
        let mut swap = None;
        {
            let gr = &mut self.scene.groups[g];
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut gr.name);
            });
            ui.horizontal(|ui| {
                if style::pill(ui, "＋ 足す", gr.op == Op::Add).clicked() {
                    gr.op = Op::Add;
                }
                if style::pill(ui, "− 削る", gr.op == Op::Subtract).clicked() {
                    gr.op = Op::Subtract;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("前のグループと\nとけあう").size(11.5).color(style::DIM));
                ui.add(egui::Slider::new(&mut gr.blend, 0.0..=60.0).suffix(" mm"));
            });
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(g > 0, egui::Button::new("↑ 前へ")).clicked() {
                swap = Some((g, g - 1));
            }
            if ui.add_enabled(g + 1 < n, egui::Button::new("↓ 後ろへ")).clicked() {
                swap = Some((g, g + 1));
            }
        });
        if let Some((a, b)) = swap {
            self.scene.groups.swap(a, b);
            self.sel = Some(Pick::Group(b));
        }
        self.actions(ui);
        ui.label(RichText::new("グループごと動かす・回す・大きさを変えられます").size(11.0).color(style::DIM));
    }

    fn shape_inspector(&mut self, ui: &mut Ui, g: usize, s: usize) {
        style::title(ui, "形");
        let ngroups = self.scene.groups.len();
        let nshapes = self.scene.groups[g].shapes.len();
        let mut move_to: Option<usize> = None;
        let mut reorder: Option<usize> = None;
        let mut outline_changed = false;
        {
            let Some(sh) = self.scene.shape_mut(g, s) else { return };
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut sh.name);
            });
            ui.horizontal(|ui| {
                if style::pill(ui, "＋ 足す", sh.op == Op::Add).clicked() {
                    sh.op = Op::Add;
                }
                if style::pill(ui, "− 削る", sh.op == Op::Subtract).clicked() {
                    sh.op = Op::Subtract;
                }
            });
            style::xyz(ui, "位置", &mut sh.pos, 0.5, -5000.0..=5000.0, "");
            style::xyz(ui, "回転", &mut sh.rot, 1.0, -3600.0..=3600.0, "°");
            style::xyz(ui, "大きさ", &mut sh.scale, 0.01, 0.01..=100.0, "×");
            let labels = sh.kind.size_labels();
            ui.horizontal_wrapped(|ui| {
                for (i, l) in labels.iter().enumerate() {
                    if !l.is_empty() {
                        ui.label(RichText::new(*l).color(style::DIM).size(12.0));
                        ui.add(egui::DragValue::new(&mut sh.size[i]).speed(0.3).range(0.1..=2000.0).suffix(" mm").max_decimals(1));
                    }
                }
            });
            if matches!(sh.kind, Kind::Box | Kind::Cylinder | Kind::Extrude) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("かどの丸み").color(style::DIM).size(12.0));
                    ui.add(egui::Slider::new(&mut sh.round, 0.0..=40.0).suffix(" mm"));
                });
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("とけあい").color(style::DIM).size(12.0));
                ui.add(egui::Slider::new(&mut sh.blend, 0.0..=60.0).suffix(" mm")).on_hover_text("この形が、まわりとどのくらい溶けてくっつくか");
            });
            if sh.kind == Kind::Extrude {
                outline_changed = true;
            }
        }
        if outline_changed {
            self.sketch_editor(ui, g, s);
        }
        self.material_panel(ui, g, s);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(s > 0, egui::Button::new("↑")).on_hover_text("ひとつ前へ").clicked() {
                reorder = Some(s - 1);
            }
            if ui.add_enabled(s + 1 < nshapes, egui::Button::new("↓")).on_hover_text("ひとつ後ろへ").clicked() {
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

    fn material_panel(&mut self, ui: &mut Ui, g: usize, s: usize) {
        style::title(ui, "ざいしつ");
        let Some(sh) = self.scene.shape_mut(g, s) else { return };
        let mut chosen: Option<Material> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
            for (name, c, r, m) in SWATCHES {
                let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                let col = Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
                let p = ui.painter();
                p.circle_filled(rect.center(), 12.0, col);
                // a little highlight: shinier materials get a brighter dot
                p.circle_filled(rect.center() + vec2(-4.0, -4.5), 3.2 * (1.2 - r), Color32::from_white_alpha((90.0 + 140.0 * (1.0 - r)) as u8));
                if m > 0.5 {
                    p.circle_stroke(rect.center(), 12.0, Stroke::new(1.2, Color32::from_white_alpha(120)));
                }
                if resp.hovered() {
                    p.circle_stroke(rect.center(), 13.5, Stroke::new(1.5, style::ACCENT));
                }
                if resp.on_hover_text(name).clicked() {
                    chosen = Some(Material { color: c, roughness: r, metallic: m });
                }
            }
        });
        let m = &mut sh.material;
        ui.horizontal(|ui| {
            ui.label(RichText::new("いろ").color(style::DIM).size(12.0));
            ui.color_edit_button_rgb(&mut m.color);
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("つや").color(style::DIM).size(12.0));
            let mut gloss = 1.0 - m.roughness;
            if ui.add(egui::Slider::new(&mut gloss, 0.0..=1.0).show_value(false)).changed() {
                m.roughness = 1.0 - gloss;
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("金属").color(style::DIM).size(12.0));
            ui.add(egui::Slider::new(&mut m.metallic, 0.0..=1.0).show_value(false));
        });
        if let Some(c) = chosen {
            // a whole group takes the material when the group is picked in the layers; here, one shape
            sh.material = c;
        }
    }

    // ----- sketch to extrude -----

    fn sketch_editor(&mut self, ui: &mut Ui, g: usize, s: usize) {
        style::title(ui, "スケッチ（輪かく）");
        let side = ui.available_width().min(240.0);
        let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        let Some(sh) = self.scene.shape_mut(g, s) else { return };
        let extent = sh.outline.iter().map(|p| p[0].abs().max(p[1].abs())).fold(20.0_f32, f32::max) * 1.25;
        let k = side * 0.5 / extent;
        let to_px = |p: [f32; 2]| Pos2::new(rect.center().x + p[0] * k, rect.center().y - p[1] * k);
        let to_mm =
            |q: Pos2| [((q.x - rect.center().x) / k).round().clamp(-2000.0, 2000.0), ((rect.center().y - q.y) / k).round().clamp(-2000.0, 2000.0)];
        let p = ui.painter();
        p.rect_filled(rect, 8.0, style::PANEL_DARK);
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
        let pts: Vec<Pos2> = sh.outline.iter().map(|q| to_px(*q)).collect();
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
            let r = ui.interact(hr, ui.id().with(("pt", g, s, i)), Sense::click_and_drag());
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
        let bg = ui.interact(rect, ui.id().with(("sk", g, s)), Sense::click());
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
            insert = Some((best.1, to_mm(m)));
        }
        if let Some((i, m)) = moved
            && let Some(p) = sh.outline.get_mut(i)
        {
            *p = m;
        }
        if let Some((i, m)) = insert
            && sh.outline.len() < MAX_OUTLINE
        {
            sh.outline.insert(i, m);
        }
        if let Some(i) = remove
            && sh.outline.len() > 3
        {
            sh.outline.remove(i);
        }
        ui.label(RichText::new(format!("線をクリックで角を足す　{}/{}個", sh.outline.len(), MAX_OUTLINE)).size(11.0).color(style::DIM));
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
            for (name, pts) in shapes {
                if style::pill(ui, name, false).clicked() {
                    sh.outline = pts.into_iter().map(|p| [p[0].round(), p[1].round()]).collect();
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
        ui.horizontal(|ui| {
            for mode in BlendMode::ALL {
                if style::pill(ui, mode.label(), m.mode == mode).clicked() {
                    m.mode = mode;
                }
            }
        });
        for (label, value, range, tip) in [
            ("とけぐあい", &mut m.amount, 0.0..=4.0, "全部の形のとけあいを一度に強く・弱く"),
            ("たれ", &mut m.drip, 0.0..=40.0, "下にたれるように溶ける"),
            ("たれの細かさ", &mut m.ripple, 0.3..=4.0, "たれの間かく"),
        ] {
            ui.horizontal(|ui| {
                ui.add_sized([74.0, 18.0], egui::Label::new(RichText::new(label).color(style::DIM).size(12.0)));
                ui.add(egui::Slider::new(value, range)).on_hover_text(tip);
            });
        }
        ui.label(RichText::new("形ごとの とけあい は「形」の中で決められます").size(11.0).color(style::DIM));
    }
}

#[derive(Default)]
struct RowResult {
    select: bool,
    eye: bool,
    op: bool,
    chevron: bool,
}

/// Two blobs melting together with the chosen curve: a live picture of what the settings do.
fn melt_preview(ui: &mut Ui, m: &Melt) {
    let w = ui.available_width().min(284.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 96.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 10.0, style::PANEL_DARK);
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
