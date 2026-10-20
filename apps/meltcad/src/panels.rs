//! The right side: tabs for the selected thing's shape and material, the melt, and the room;
//! plus the scene list shown on the left.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, Tab};
use crate::materials::{self, Category};
use crate::model::{BlendMode, Corner, Kind, Light, MAX_LIGHTS, MAX_OUTLINE, MAX_SAVED, Material, Melt, Op, Pick, ROUND_MAX};
use crate::sdf;
use crate::{glyphs, style};

/// (label, mode, amount, drip, ripple)
const MELT_PRESETS: [(&str, BlendMode, f32, f32, f32); 6] = [
    ("Firm", BlendMode::Smooth, 0.0, 0.0, 1.0),
    ("Soft", BlendMode::Smooth, 1.0, 0.0, 1.0),
    ("Bouncy", BlendMode::Round, 1.6, 0.0, 1.0),
    ("Cheese", BlendMode::Gooey, 1.0, 8.0, 1.0),
    ("Honey", BlendMode::Gooey, 1.5, 16.0, 0.7),
    ("Runny", BlendMode::Gooey, 2.4, 28.0, 1.0),
];

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
                let tabs = [(Tab::Shape, "Shape"), (Tab::Material, "Material"), (Tab::Melt, "Melt"), (Tab::Room, "Light")];
                let items: Vec<(&str, &str)> = tabs.iter().map(|t| (t.1, "")).collect();
                let sel = tabs.iter().position(|t| t.0 == self.tab);
                if let Some(i) = style::segmented(ui, "side_tabs", &items, sel, true)
                    && let Some(t) = tabs.get(i)
                {
                    self.tab = t.0;
                }
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Shape => {
                        let locked = self.scene_structure_locked();
                        if locked {
                            ui.label(
                                RichText::new("Use the active command card. Finish or cancel to edit other shapes.").size(12.0).color(style::DIM),
                            );
                            ui.add_space(6.0);
                        }
                        ui.add_enabled_ui(!locked, |ui| self.inspector(ui));
                    }
                    Tab::Material => self.material_tab(ui),
                    Tab::Melt => self.melt_panel(ui),
                    Tab::Room => self.room_panel(ui),
                });
            });
    }

    /// The menu on a right click in the viewport.
    pub fn context_menu(&mut self, ui: &mut Ui) {
        if self.sel.is_none() {
            ui.label(RichText::new("Nothing selected").color(style::DIM));
            return;
        }
        let editable = !self.scene_structure_locked();
        if ui.add_enabled(editable, egui::Button::new("Duplicate  Ctrl+D")).clicked() {
            self.duplicate();
            ui.close();
        }
        if ui.button("Add ⇄ Subtract  Tab").clicked() {
            self.flip_op();
            ui.close();
        }
        if ui.button("Focus  F").clicked() {
            self.frame_view(true);
            ui.close();
        }
        if ui.button("Rename").clicked() {
            self.renaming = self.sel;
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(editable, egui::Button::new(RichText::new("Delete  Del").color(style::CUT))).clicked() {
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
        let badges: Vec<String> = if self.scene.groups.iter().any(|g| g.pattern.is_some() || g.is_linked() || g.piece.is_some()) {
            let resolved = crate::pattern::resolve(&self.scene);
            (0..ngroups)
                .map(|gi| {
                    let g = &self.scene.groups[gi];
                    let n = resolved.iter().find(|r| r.gi == gi).map_or(0, |r| r.insts.len().max(1));
                    match (g.is_linked(), g.pattern.is_some()) {
                        (true, true) => format!("Linked ×{n}"),
                        (true, false) => "Linked".to_string(),
                        (false, true) => format!("×{n}"),
                        _ if g.piece.is_some() => "File".to_string(),
                        _ => String::new(),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        // a folded group hides everything nested in it
        let mut hidden_below: Option<u8> = None;
        // Shift + click: everything between the picked row and this one (Ctrl + click adds one by one)
        let mut range_to: Option<Pick> = None;
        for gi in 0..ngroups {
            let depth = self.scene.groups[gi].depth;
            if let Some(d) = hidden_below {
                if depth > d {
                    continue;
                }
                hidden_below = None;
            }
            let indent_px = f32::from(depth) * 14.0;
            let folded = self.folded.contains(&gi);
            if folded {
                hidden_below = Some(depth);
            }
            let sel = self.picks().contains(&Pick::Group(gi));
            let (name, op, visible, nshapes) = {
                let g = &self.scene.groups[gi];
                (g.name.clone(), g.op, g.visible, g.shapes.len())
            };
            let id = ui.id().with(("g", gi));
            let badge = badges.get(gi).cloned().unwrap_or_default();
            let row = self.row(ui, id, sel, indent_px, name, Pick::Group(gi), op, visible, glyphs::group, !folded, true, &badge);
            if row.chevron {
                if folded {
                    self.folded.remove(&gi);
                } else {
                    self.folded.insert(gi);
                }
            }
            if row.select {
                let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command || i.modifiers.ctrl));
                if ctrl {
                    self.toggle_pick(Pick::Group(gi));
                } else if shift {
                    range_to = Some(Pick::Group(gi));
                } else {
                    self.sel = Some(Pick::Group(gi));
                }
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
                    let sel = self.picks().contains(&Pick::Shape(gi, si));
                    let id = ui.id().with(("s", gi, si));
                    let row = self.row(
                        ui,
                        id,
                        sel,
                        18.0 + indent_px,
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
                        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command || i.modifiers.ctrl));
                        if ctrl {
                            self.toggle_pick(Pick::Shape(gi, si));
                        } else if shift {
                            range_to = Some(Pick::Shape(gi, si));
                        } else {
                            self.sel = Some(Pick::Shape(gi, si));
                        }
                    }
                    if row.drag_started {
                        self.layer_drag = Some(Pick::Shape(gi, si));
                    }
                    if row.eye
                        && let Some(s) = self.scene.shape_mut(gi, si)
                    {
                        s.visible = !visible;
                    }
                    if row.op {
                        if self.sk.live == Some((gi, si)) {
                            self.flip_op();
                        } else if let Some(s) = self.scene.shape_mut(gi, si) {
                            s.op = op.next();
                        }
                    }
                    self.curve_ball_rows(ui, gi, si, 36.0 + indent_px);
                }
            }
        }
        if let Some(to) = range_to {
            let order: Vec<Pick> = self.row_rects.iter().map(|(k, _)| *k).collect();
            let from = self.sel.filter(|s| order.contains(s)).unwrap_or(to);
            let (i, j) = (order.iter().position(|k| *k == from), order.iter().position(|k| *k == to));
            if let (Some(i), Some(j)) = (i, j) {
                let (lo, hi) = (i.min(j), i.max(j));
                self.sel = Some(from);
                self.multi = order[lo..=hi].iter().copied().filter(|k| *k != from).collect();
                self.multi_for = self.sel;
            }
        }
        if ngroups == 0 {
            ui.label(RichText::new("Click a shape above to add one").color(style::DIM).size(12.0));
        }
        self.layer_drop(ui);
    }

    /// A selected solid curve is a group of balls: one small row per ball, to pick it from the list.
    /// Saved sketches use Edit Sketch and do not expose their display stroke as radius-bearing balls.
    fn curve_ball_rows(&mut self, ui: &mut Ui, gi: usize, si: usize, indent: f32) {
        if self.sel != Some(Pick::Shape(gi, si)) {
            return;
        }
        let Some(sh) = self.scene.shape(gi, si) else { return };
        if sh.guide || !sh.kind.uses_path() {
            return;
        }
        let radii: Vec<f32> = sh.path.iter().take(256).map(|p| p[3]).collect();
        let chosen = self.chosen_node(gi, si);
        for (i, r) in radii.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            let on = chosen == Some(i);
            style::row_bg(ui, &resp, rect, on);
            let c = egui::pos2(rect.left() + indent + 8.0, rect.center().y);
            let col = if on { style::ACCENT } else { style::DIM };
            ui.painter().circle_stroke(c, 5.0, Stroke::new(1.5, col));
            let text = format!("Ball {}   r {:.1}", i + 1, r);
            ui.painter().text(
                egui::pos2(c.x + 14.0, c.y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(12.0),
                if on { style::TEXT } else { style::DIM },
            );
            if resp.clicked() {
                self.ct.node = Some((gi, si, i));
            }
        }
    }

    /// While a row is being dragged: show where it would land, and put it there on release.
    fn layer_drop(&mut self, ui: &mut Ui) {
        if self.scene_structure_locked() {
            self.layer_drag = None;
            return;
        }
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
        let locked = self.scene_structure_locked();
        let sense = if locked { Sense::click() } else { Sense::click_and_drag() };
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), sense);
        let resp = if locked { resp.on_hover_text("Finish or cancel the active command before reordering") } else { resp };
        self.row_rects.push((pick, rect));
        let mut out = RowResult::default();
        style::row_bg(ui, &resp, rect, selected);
        // the grip says "drag me"
        let grip = Color32::from_white_alpha(if locked {
            16
        } else if resp.hovered() || selected {
            120
        } else {
            44
        });
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
            // take the keyboard once when the rename starts; after that a click elsewhere ends it
            let started = id.with("rename_started");
            if !ui.data(|d| d.get_temp::<bool>(started).unwrap_or(false)) {
                r.request_focus();
                ui.data_mut(|d| d.insert_temp(started, true));
            }
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
            if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.renaming = None;
                ui.data_mut(|d| d.remove::<bool>(started));
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
        out.drag_started = !locked && resp.drag_started() && !out.eye && !out.op;
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
                style::title(ui, "Selection");
                ui.label(RichText::new("Click a shape in the view or the scene list").color(style::DIM).size(12.0));
            }
        }
    }

    fn actions(&mut self, ui: &mut Ui) {
        if self.picks().len() > 1 {
            ui.label(RichText::new(format!("{} selected (Shift + click adds or removes)", self.picks().len())).size(12.0).color(style::ACCENT));
            if style::pill(ui, "Group them (Ctrl+G)", false)
                .on_hover_text("Put the selected shapes or groups inside one new group (groups can nest)")
                .clicked()
            {
                self.group_selected();
            }
        }
        ui.horizontal(|ui| {
            if style::icon_button(ui, 34.0, "Duplicate (Ctrl+D)", false, glyphs::dup).clicked() {
                self.duplicate();
            }
            if style::icon_button(ui, 34.0, "Delete (Del)", false, glyphs::trash).clicked() {
                self.delete_selected();
            }
            if matches!(self.sel, Some(Pick::Shape(..)))
                && style::pill(ui, "Mirror copy", false).on_hover_text("Copy to the opposite side (ears, eyes, arms)").clicked()
            {
                self.mirror_copy();
            }
        });
    }

    fn op_pills(ui: &mut Ui, op: &mut Op) {
        let ops = [
            (Op::Add, "＋ Add", "Join this shape with the others"),
            (Op::Subtract, "− Subtract", "Cut a hole with this shape"),
            (Op::Paint, "● Paint", "Change only the color and material, not the shape"),
            (Op::Intersect, "◐ Overlap", "Keep only the part that overlaps the shape before"),
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
        style::title(ui, "Group");
        let parent = self.scene.parent_of(g).and_then(|p| self.scene.groups.get(p)).map(|p| p.name.clone());
        {
            let id = egui::Id::new(("group_melt_memory", g));
            let gr = &mut self.scene.groups[g];
            ui.text_edit_singleline(&mut gr.name);
            Self::op_pills(ui, &mut gr.op);
            // Melt on / off, and how much: off keeps the group a hard shape next to the others
            let mut on = gr.blend > 0.0;
            let was = on;
            if style::pill(ui, if on { "Melt: on" } else { "Melt: off" }, on)
                .on_hover_text("Off: this group stays a sharp, separate shape. On: it melts into what is next to it")
                .clicked()
            {
                on = !on;
            }
            if on != was {
                if on {
                    gr.blend = ui.ctx().data(|d| d.get_temp::<f32>(id)).unwrap_or(12.0).max(1.0);
                } else {
                    ui.ctx().data_mut(|d| d.insert_temp(id, gr.blend));
                    gr.blend = 0.0;
                }
            }
            if gr.blend > 0.0 {
                let into = parent.as_deref().map_or("the scene".to_string(), |p| format!("group “{p}”"));
                style::labeled_slider(
                    ui,
                    "Melt amount",
                    &mut gr.blend,
                    0.0..=80.0,
                    &format!("How much this whole group melts into the groups before it inside {into}"),
                );
            }
        }
        self.piece_card(ui, g);
        // what it melts with: the shapes inside it melt together, and the groups at the same level melt with each other
        let into = parent.clone().map_or("Scene".to_string(), |p| p);
        let partners: Vec<String> = (0..self.scene.groups.len())
            .filter(|&i| i != g && self.scene.groups[i].depth == self.scene.groups[g].depth && self.scene.parent_of(i) == self.scene.parent_of(g))
            .map(|i| self.scene.groups[i].name.clone())
            .collect();
        ui.label(
            RichText::new(format!(
                "Melts with: {} (inside {into})",
                if partners.is_empty() { "nothing else".to_string() } else { partners.join(", ") }
            ))
            .size(12.0)
            .color(style::DIM),
        );
        let mut reselect = None;
        ui.horizontal_wrapped(|ui| {
            if style::pill_enabled(ui, "↑", false, self.scene.prev_sibling(g).is_some()).on_hover_text("Move up").clicked() {
                reselect = self.scene.shift_group(g, false);
            }
            if style::pill_enabled(ui, "↓", false, self.scene.next_sibling(g).is_some()).on_hover_text("Move down").clicked() {
                reselect = self.scene.shift_group(g, true);
            }
            if style::pill_enabled(ui, "→ Into group above", false, g > 0)
                .on_hover_text("Put this group inside the group above it, so they melt as one")
                .clicked()
                && self.scene.indent_group(g)
            {
                reselect = Some(g);
            }
            if style::pill_enabled(ui, "← Out", false, parent.is_some()).on_hover_text("Take this group out of its parent group").clicked() {
                reselect = self.scene.outdent_group(g);
            }
            if style::pill(ui, "Wrap in new group", false).on_hover_text("Make a new parent group around this one").clicked() {
                let name = format!("Group {}", self.scene.groups.len() + 1);
                reselect = self.scene.wrap_in_group(g, name);
            }
        });
        if let Some(i) = reselect {
            self.sel = Some(Pick::Group(i));
            self.folded.clear();
        }
        self.pattern_section(ui, g);
        self.actions(ui);
        let linked = self.scene.groups.get(g).is_some_and(crate::model::Group::is_linked);
        let hint = if linked { "Linked copy: set position, rotation and scale above" } else { "Move, rotate and scale the whole group" };
        ui.label(RichText::new(hint).size(12.0).color(style::DIM));
    }

    fn shape_inspector(&mut self, ui: &mut Ui, g: usize, s: usize) {
        if self.scene.shape(g, s).is_some_and(|shape| shape.guide) {
            self.guide_inspector(ui, g, s);
            return;
        }
        style::title(ui, "Shape");
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
                    ui.label(RichText::new("Kind").color(style::DIM).size(12.0));
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for k in Kind::ALL {
                        let on = sh.kind == k;
                        if style::icon_button(ui, 32.0, k.label(), on, |p, r, ink| glyphs::kind(p, r, k, ink, Color32::from_white_alpha(18)))
                            .clicked()
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
            style::xyz(ui, "Position", &mut sh.pos, 0.5, -5000.0..=5000.0, "");
            style::xyz(ui, "Rotation", &mut sh.rot, 1.0, -3600.0..=3600.0, "°");
            style::xyz(ui, "Scale", &mut sh.scale, 0.01, 0.01..=100.0, "×");
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
                style::labeled_slider(ui, "Round", &mut sh.round, 0.0..=ROUND_MAX, "Corner and edge radius (mm). Stops before the shape breaks");
                let styles = [("Fillet", "Round off the corners"), ("Chamfer", "Cut the corners flat at an angle")];
                let now = Corner::ALL.iter().position(|c| *c == sh.corner);
                if let Some(i) = style::segmented(ui, "corner_style", &styles, now, true) {
                    sh.corner = Corner::ALL.get(i).copied().unwrap_or_default();
                }
            }
            {
                let on = sh.blend > 0.0;
                let id = egui::Id::new(("shape_melt_memory", g, s));
                if style::pill(ui, if on { "Melt: on" } else { "Melt: off" }, on)
                    .on_hover_text("Off: this shape stays sharp. On: it melts into the other shapes of its group")
                    .clicked()
                {
                    if on {
                        ui.ctx().data_mut(|d| d.insert_temp(id, sh.blend));
                        sh.blend = 0.0;
                    } else {
                        sh.blend = ui.ctx().data(|d| d.get_temp::<f32>(id)).unwrap_or(6.0).max(1.0);
                    }
                }
                if sh.blend > 0.0 {
                    style::labeled_slider(
                        ui,
                        "Melt amount",
                        &mut sh.blend,
                        0.0..=80.0,
                        "How much this shape melts into the other shapes of its group",
                    );
                }
            }
            style::labeled_slider(ui, "Shell", &mut sh.shell, 0.0..=40.0, "Hollow out the inside, leaving walls this thick (mm). 0 = solid");
            // melt just one face: the rest keeps its exact size
            {
                let fm = &mut sh.face_melt;
                ui.label(RichText::new("Melt one face").color(style::DIM).size(12.0));
                let mut items: Vec<(&str, &str)> = vec![("None", "Whole shape keeps its exact size")];
                items.extend(crate::model::FaceMelt::FACES.iter().copied());
                let now = Some(usize::from(fm.face.min(6)));
                if let Some(i) = style::segmented(ui, "face_melt_face", &items, now, true) {
                    fm.face = i.min(6) as u8;
                    if fm.face > 0 {
                        if fm.amount == 0.0 {
                            fm.amount = 4.0;
                        }
                        if fm.reach < 1.0 {
                            fm.reach = 12.0;
                        }
                    }
                }
                if fm.face > 0 {
                    style::labeled_slider(
                        ui,
                        "Swell",
                        &mut fm.amount,
                        -30.0..=30.0,
                        "How far this face swells out (+) or sinks in (-), in mm; other faces keep their size",
                    );
                    style::labeled_slider(ui, "Depth", &mut fm.reach, 2.0..=60.0, "How far the melt reaches from the face, in mm");
                }
            }
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
            if style::pill_enabled(ui, "↑", false, s > 0).on_hover_text("Move up one").clicked() {
                reorder = Some(s - 1);
            }
            if style::pill_enabled(ui, "↓", false, s + 1 < nshapes).on_hover_text("Move down one").clicked() {
                reorder = Some(s + 1);
            }
            if ngroups > 1 {
                egui::ComboBox::from_id_salt("move_to").selected_text("Move to group").show_ui(ui, |ui| {
                    for (i, gr) in self.scene.groups.iter().enumerate() {
                        if i != g && ui.selectable_label(false, &gr.name).clicked() {
                            move_to = Some(i);
                        }
                    }
                });
            }
        });
        if let Some(t) = reorder {
            self.move_layer(Pick::Shape(g, s), Pick::Shape(g, t), t > s);
        }
        if let Some(t) = move_to {
            self.move_layer(Pick::Shape(g, s), Pick::Group(t), false);
        }
        self.actions(ui);
    }

    /// A sketch is a source for solid tools, so its inspector shows those actions
    /// and its planar size instead of solid-only radius, shell and melt controls.
    fn guide_inspector(&mut self, ui: &mut Ui, g: usize, s: usize) {
        style::title(ui, "Sketch");
        let profile = self.curve_profile(g, s);
        let closed = profile.is_some();
        if let Some(sh) = self.scene.shape_mut(g, s) {
            ui.text_edit_singleline(&mut sh.name);
            style::checkbox(ui, &mut sh.visible, "Visible");
            ui.label(RichText::new(if closed { "Closed profile" } else { "Open path" }).color(style::DIM));
            style::xyz(ui, "Position", &mut sh.pos, 0.5, -5000.0..=5000.0, "");
            style::xyz(ui, "Rotation", &mut sh.rot, 1.0, -3600.0..=3600.0, "°");
            style::xyz(ui, "Scale", &mut sh.scale, 0.01, 0.01..=100.0, "×");
        }
        if let Some((_, points)) = &profile
            && let Some((lo, hi)) = crate::sketch::bbox(points)
        {
            ui.label(format!("{:.2} × {:.2} mm  ·  {} points", hi[0] - lo[0], hi[1] - lo[1], points.len()));
        }
        if style::pill_enabled(ui, "Edit Sketch", false, closed && !self.sketch_active())
            .on_hover_text("Edit dimensions; right-click saves the sketch, Esc cancels the draft")
            .clicked()
        {
            self.edit_saved_sketch(g, s);
        }
        ui.separator();
        ui.label(RichText::new("Create solid").strong());
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            for make in crate::sketch_ui::Make::ALL {
                let enabled = !self.sketch_active() && (closed || make == crate::sketch_ui::Make::Sweep);
                if style::pill_enabled(ui, make.label(), false, enabled).on_hover_text("Adjust the preview, then right-click to confirm").clicked() {
                    self.sel = Some(Pick::Shape(g, s));
                    self.make_from_selected_line(&ctx, make);
                }
            }
        });
        ui.add_space(6.0);
        egui::CollapsingHeader::new("Edit path points").show(ui, |ui| self.curve_inspector(ui, g, s));
        self.actions(ui);
    }

    // ----- materials -----

    fn material_tab(&mut self, ui: &mut Ui) {
        let mut tg = self.sel_targets();
        if self.mat_group
            && let Some(Pick::Shape(g, _)) = self.sel
            && let Some(gr) = self.scene.groups.get(g)
        {
            tg = (0..gr.shapes.len()).map(|s| (g, s)).collect();
        }
        let Some(&(g0, s0)) = tg.first() else {
            style::title(ui, "Material");
            ui.label(RichText::new("Select a shape to choose its material here").color(style::DIM).size(12.0));
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
            style::gfx::shadow(
                &ui.painter_at(rect),
                Rect::from_center_size(rect.center() + vec2(0.0, 24.0), vec2(36.0, 4.0)),
                2.0,
                &[(0, 10, 0, 90)],
            );
            materials::swatch(&ui.painter_at(rect), rect.center(), 28.0, &m);
            ui.vertical(|ui| {
                ui.label(RichText::new(current.unwrap_or("Custom")).strong().size(14.0));
                ui.label(RichText::new(format!("Weight: {}", materials::weight_word(&m))).size(11.5).color(style::DIM))
                    .on_hover_text("How hard this material is for the graphics card. Glass, soaking light, coat and sheen are heavy");
                ui.horizontal(|ui| {
                    let save_reason = if self.scene.saved.contains(&m) {
                        Some("Already saved under ★ My")
                    } else if self.scene.saved.len() >= MAX_SAVED {
                        Some("Save limit reached. Right-click a saved material to remove it")
                    } else {
                        None
                    };
                    let save = style::pill_enabled(ui, "★ Save", false, save_reason.is_none());
                    if let Some(reason) = save_reason {
                        save.on_hover_text(reason);
                    } else if save.clicked() {
                        self.scene.saved.push(m);
                        self.mat_cat = None;
                    }
                });
                if matches!(self.sel, Some(Pick::Shape(..))) {
                    style::checkbox(ui, &mut self.mat_group, RichText::new("Apply to whole group").size(12.5));
                }
            });
        });
        ui.add_space(4.0);

        // lighter, and random colours
        let mut all_light = false;
        let mut rand_sel = false;
        let mut rand_all = false;
        let mut light_sel = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            light_sel = style::pill(ui, "Lighter", false)
                .on_hover_text("Switch off see-through, soaking, coat and sheen on the selected shapes: much faster")
                .clicked();
            all_light = style::pill(ui, "All lighter", false).on_hover_text("Make every material in the scene light").clicked();
            rand_sel = style::pill(ui, "Random color", false).on_hover_text("A new random color for each selected shape").clicked();
            rand_all = style::pill(ui, "Random all", false).on_hover_text("A random color for every shape").clicked();
        });
        let mut seed = materials::fresh_seed();
        if light_sel || rand_sel {
            for &(g, s) in &tg {
                if let Some(sh) = self.scene.shape_mut(g, s) {
                    if light_sel {
                        sh.material = materials::lighter(&sh.material);
                    }
                    if rand_sel {
                        sh.material.color = materials::random_color(&mut seed);
                    }
                }
            }
        }
        if all_light || rand_all {
            for gr in &mut self.scene.groups {
                for sh in &mut gr.shapes {
                    if all_light {
                        sh.material = materials::lighter(&sh.material);
                    }
                    if rand_all {
                        sh.material.color = materials::random_color(&mut seed);
                    }
                }
            }
        }
        if (light_sel || rand_sel || all_light || rand_all)
            && let Some(sh) = self.scene.shape(g0, s0)
        {
            m = sh.material;
        }
        let first = m;

        // categories
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for c in Category::ALL {
                if style::pill(ui, c.label(), self.mat_cat == Some(c)).clicked() {
                    self.mat_cat = Some(c);
                }
            }
            if style::pill(ui, "★ My", self.mat_cat.is_none()).clicked() {
                self.mat_cat = None;
            }
            if style::pill(ui, "Light first", self.mat_sort).on_hover_text("Order the materials from lightest to heaviest").clicked() {
                self.mat_sort = !self.mat_sort;
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
                    let mut list: Vec<&materials::Preset> = presets.iter().filter(|p| p.category == cat).collect();
                    if self.mat_sort {
                        list.sort_by(|a, b| materials::cost(&a.material).total_cmp(&materials::cost(&b.material)));
                    }
                    for pr in list {
                        let name = format!("{} ({})", pr.name, materials::weight_word(&pr.material));
                        if cell(ui, &name, &pr.material, pr.material == m).clicked() {
                            chosen = Some(pr.material);
                        }
                    }
                }
                None => {
                    if self.scene.saved.is_empty() {
                        ui.label(RichText::new("Use ★ Save to keep materials here").size(12.0).color(style::DIM));
                    }
                    for (i, sm) in self.scene.saved.iter().enumerate() {
                        let r = cell(ui, "Click to use / right-click to remove", sm, *sm == m);
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
        style::title(ui, "Details");
        ui.horizontal(|ui| {
            style::row_label(ui, "Color");
            style::color_edit(ui, &mut m.color);
        });
        let mut gloss = 1.0 - m.roughness;
        if style::labeled_slider(ui, "Gloss", &mut gloss, 0.0..=1.0, "Smooth ⇄ Rough") {
            m.roughness = 1.0 - gloss;
        }
        style::labeled_slider(ui, "Metallic", &mut m.metallic, 0.0..=1.0, "Plastic ⇄ Metal");
        style::labeled_slider(ui, "Transparent", &mut m.glass, 0.0..=1.0, "Bends light like glass and lets you see through");
        style::labeled_slider(ui, "Glow", &mut m.glow, 0.0..=4.0, "Emits its own light (lamp, neon)");
        style::labeled_slider(ui, "Iridescent", &mut m.iridescence, 0.0..=1.0, "Shifts rainbow colors with the viewing angle (soap bubble, beetle)");
        style::labeled_slider(ui, "Coat", &mut m.coat, 0.0..=1.0, "Glossy clear layer (car paint, candy)");
        style::labeled_slider(ui, "Translucency", &mut m.translucency, 0.0..=1.0, "Light soaks in (wax, jelly, ice)");
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
        style::title(ui, "Look");
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
        style::title(ui, "Backdrop");
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
            style::switch(ui, &mut e.grid, "Grid");
            style::switch(ui, &mut e.floor, "Floor shadow");
        });
        style::labeled_slider(ui, "Exposure", &mut e.exposure, 0.3..=3.0, "Brightness, like camera exposure");
        style::labeled_slider(ui, "Shadow strength", &mut e.shadow, 0.0..=1.0, "");
        style::labeled_slider(ui, "Ambient", &mut e.ambient, 0.0..=2.0, "Brightness of shadows and sky reflections");
        style::title(ui, "Sky (reflected in metal and glass)");
        ui.horizontal(|ui| {
            for (label, c) in [("Top", &mut e.sky_top), ("Horizon", &mut e.sky_horizon), ("Bottom", &mut e.sky_bottom)] {
                ui.label(RichText::new(label).size(12.0).color(style::DIM));
                style::color_edit(ui, c);
            }
        });
        style::title(ui, "Light");
        let mut remove = None;
        let n = e.lights.len();
        for (i, l) in e.lights.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Light {}", i + 1)).size(12.5).strong());
                style::color_edit(ui, &mut l.color);
                if n > 1 && ui.small_button("Remove").on_hover_text("Remove this light").clicked() {
                    remove = Some(i);
                }
            });
            style::labeled_slider(ui, "Direction", &mut l.yaw, -180.0..=180.0, "Turn around (degrees)");
            style::labeled_slider(ui, "Elevation", &mut l.pitch, 0.0..=89.0, "Low ⇄ Overhead (degrees)");
            style::labeled_slider(ui, "Power", &mut l.power, 0.0..=3.0, "");
            if i == 0 {
                ui.label(RichText::new("Only Light 1 casts shadows").size(12.0).color(style::DIM));
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove {
            e.lights.remove(i);
        }
        if e.lights.len() < MAX_LIGHTS && style::pill(ui, "+ Add light", false).clicked() {
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
                (true, _, _) => "Profile (half)",
                (_, true, false) => "Bottom outline",
                (_, true, true) => "Top outline",
                _ => "Sketch (outline)",
            },
        );
        if revolve {
            ui.label(RichText::new("X = distance from the center axis, Y = height. Revolves around the axis").size(11.0).color(style::DIM));
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
            r.on_hover_text("Drag to move / double-click to remove");
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
        ui.label(RichText::new(format!("Click an edge to add a corner  {}/{}", ol.len(), MAX_OUTLINE)).size(12.0).color(style::DIM));
        ui.horizontal_wrapped(|ui| {
            let shapes: [(&str, Vec<[f32; 2]>); 5] = [
                ("Square", vec![[-15.0, -15.0], [15.0, -15.0], [15.0, 15.0], [-15.0, 15.0]]),
                ("Triangle", vec![[-17.0, -12.0], [17.0, -12.0], [0.0, 18.0]]),
                (
                    "Hexagon",
                    (0..6)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::PI / 3.0;
                            [18.0 * a.cos(), 18.0 * a.sin()]
                        })
                        .collect(),
                ),
                (
                    "Star",
                    (0..10)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::PI / 5.0 + std::f32::consts::FRAC_PI_2;
                            let r = if i % 2 == 0 { 20.0 } else { 8.5 };
                            [r * a.cos(), r * a.sin()]
                        })
                        .collect(),
                ),
                (
                    "Cross",
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
                    ("Vase", Kind::Revolve.default_outline()),
                    ("Cup", vec![[0.0, -20.0], [14.0, -20.0], [18.0, 20.0], [16.0, 20.0], [12.0, -17.0], [0.0, -17.0]]),
                    ("Top", vec![[0.0, -22.0], [4.0, -16.0], [20.0, 0.0], [20.0, 6.0], [6.0, 8.0], [5.0, 22.0], [0.0, 22.0]]),
                    ("Square cylinder", vec![[0.0, -20.0], [15.0, -20.0], [15.0, 20.0], [0.0, 20.0]]),
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
        style::title(ui, "Melt");
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
            ("Amount", &mut m.amount, 0.0..=4.0, "Melt strength for all shapes at once"),
            ("Drip", &mut m.drip, 0.0..=40.0, "Melts downward, like dripping"),
            ("Drip spacing", &mut m.ripple, 0.3..=4.0, "Spacing of the drips"),
        ] {
            style::labeled_slider(ui, label, value, range, tip);
        }
        ui.label(RichText::new("Melt per shape is set in the Shape tab").size(12.0).color(style::DIM));
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
    p.text(rect.left_bottom() + vec2(8.0, -6.0), Align2::LEFT_BOTTOM, "Preview", FontId::proportional(10.0), Color32::from_white_alpha(70));
}

/// The gradient part of the material tab: a switch, the second colour, the direction and the blend.
fn gradient_controls(ui: &mut Ui, m: &mut Material) {
    style::title(ui, "Gradient");
    let mut on = m.has_gradient();
    if style::switch(ui, &mut on, "Gradient").on_hover_text("The two colors blend smoothly").changed() {
        m.gradient = if on { 1.0 } else { 0.0 };
    }
    if !on {
        return;
    }
    ui.horizontal(|ui| {
        style::row_label(ui, "Second color");
        style::color_edit(ui, &mut m.color2);
    });
    let dirs = [("Left-Right", "X: left → right"), ("Front-Back", "Y: front → back"), ("Up-Down", "Z: bottom → top")];
    if let Some(i) = style::segmented(ui, "grad_axis", &dirs, Some(usize::from(m.grad_axis.min(2))), true) {
        m.grad_axis = i as u8;
    }
    ui.horizontal(|ui| {
        style::switch(ui, &mut m.grad_flip, "Flip").on_hover_text("Reverse the color direction");
        style::switch(ui, &mut m.grad_local, "Follow shape").on_hover_text("On: the color stays on the shape when you rotate or stretch it");
    });
    style::labeled_slider(ui, "Softness", &mut m.grad_soft, 0.0..=1.0, "0 = sharp two colors ⇄ 1 = slow blend");
    style::labeled_slider(ui, "Strength", &mut m.gradient, 0.05..=1.0, "How much of the end color shows");
    style::labeled_slider(ui, "From", &mut m.grad_from, 0.0..=1.0, "Where the color starts to change (0 = edge)");
    style::labeled_slider(ui, "To", &mut m.grad_to, 0.0..=1.0, "Where the color finishes changing (1 = opposite edge)");
}
