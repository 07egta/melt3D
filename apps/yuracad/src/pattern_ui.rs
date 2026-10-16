//! The group inspector's "パターン" section, and the commands behind the Edit menu and the search
//! palette: live patterns (mirror, grid, circle, along a path, on points) and linked copies
//! ("インスタンス") of a group. The numbers and the maths are in `pattern.rs`.

use egui::{Color32, Painter, Rect, RichText, Stroke, Ui, pos2, vec2};

use crate::app::App;
use crate::model::{Group, MAX_GROUPS, Pick};
use crate::pattern::{self, PathMode, Pattern, PatternKind, Source, Xform};
use crate::style;

// ---------------------------------------------------------------------------------------------
// Icons

fn frame(rect: Rect) -> Rect {
    let s = rect.width().min(rect.height()) * 0.8;
    Rect::from_center_size(rect.center(), vec2(s, s))
}

fn at(f: Rect, u: f32, v: f32) -> egui::Pos2 {
    pos2(f.left() + f.width() * u, f.top() + f.height() * v)
}

/// A small line icon for a kind of pattern (drawn in code, like the other icons).
pub fn icon(kind: PatternKind, p: &Painter, rect: Rect, ink: Color32) {
    let f = frame(rect);
    let pen = Stroke::new((rect.width() * 0.075).clamp(1.2, 2.4), ink);
    let dot = |u: f32, v: f32, r: f32| p.circle_filled(at(f, u, v), f.width() * r, ink);
    match kind {
        PatternKind::Mirror => {
            // a dashed mirror line, a shape and its reflection
            for k in 0..4 {
                let a = 0.06 + k as f32 * 0.25;
                p.line_segment([at(f, 0.5, a), at(f, 0.5, a + 0.12)], Stroke::new(pen.width * 0.8, ink.gamma_multiply(0.7)));
            }
            let left = vec![at(f, 0.40, 0.25), at(f, 0.40, 0.78), at(f, 0.08, 0.78)];
            let right = vec![at(f, 0.60, 0.25), at(f, 0.60, 0.78), at(f, 0.92, 0.78)];
            p.add(egui::Shape::closed_line(left, pen));
            p.add(egui::Shape::convex_polygon(right, ink.gamma_multiply(0.3), pen));
        }
        PatternKind::Rect => {
            for i in 0..3 {
                for j in 0..3 {
                    let (u, v) = (0.18 + i as f32 * 0.32, 0.18 + j as f32 * 0.32);
                    p.rect_filled(Rect::from_center_size(at(f, u, v), vec2(f.width() * 0.2, f.height() * 0.2)), 2.0, if i + j == 0 { ink } else { ink.gamma_multiply(0.6) });
                }
            }
        }
        PatternKind::Circular => {
            for k in 0..6 {
                let a = k as f32 * std::f32::consts::TAU / 6.0;
                dot(0.5 + 0.34 * a.cos(), 0.5 + 0.34 * a.sin(), if k == 0 { 0.11 } else { 0.08 });
            }
            p.circle_stroke(at(f, 0.5, 0.5), f.width() * 0.34, Stroke::new(pen.width * 0.6, ink.gamma_multiply(0.45)));
            p.circle_filled(at(f, 0.5, 0.5), f.width() * 0.04, ink.gamma_multiply(0.7));
        }
        PatternKind::Path => {
            let pts: Vec<egui::Pos2> = (0..=16)
                .map(|i| {
                    let u = i as f32 / 16.0;
                    at(f, 0.06 + 0.88 * u, 0.5 - 0.3 * (u * std::f32::consts::TAU).sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(pen.width * 0.7, ink.gamma_multiply(0.6))));
            for u in [0.06, 0.35, 0.65, 0.94] {
                dot(u, 0.5 - 0.3 * ((u - 0.06) / 0.88 * std::f32::consts::TAU).sin(), 0.09);
            }
        }
        PatternKind::Geometry => {
            let tri = vec![at(f, 0.5, 0.12), at(f, 0.9, 0.84), at(f, 0.1, 0.84)];
            p.add(egui::Shape::closed_line(tri, Stroke::new(pen.width * 0.7, ink.gamma_multiply(0.6))));
            dot(0.5, 0.12, 0.1);
            dot(0.9, 0.84, 0.1);
            dot(0.1, 0.84, 0.1);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The inspector

/// What the user picked from a list of sources this frame.
enum Chosen {
    Shape(usize, usize),
    Group(usize),
}

impl App {
    /// The group the selection is in (a picked shape's group, or the picked group).
    pub fn selected_group(&self) -> Option<usize> {
        match self.sel {
            Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) if g < self.scene.groups.len() => Some(g),
            _ => None,
        }
    }

    /// A new pattern of `kind` on the selected group (Path and Geometry start on the first shape
    /// that has something to follow).
    pub fn set_pattern_kind(&mut self, ctx: &egui::Context, kind: PatternKind) {
        let Some(g) = self.selected_group() else {
            self.say(ctx, "グループか形をえらんでください");
            return;
        };
        let mut p = Pattern::starting(kind);
        match &mut p {
            Pattern::Path { source, .. } => {
                let found = self.scene.groups.iter().enumerate().find_map(|(gi, gr)| gr.shapes.iter().position(|s| pattern::shape_polyline(s).is_some()).map(|si| (gi, si)));
                match found.and_then(|(gi, si)| self.scene.ensure_shape_uid(gi, si)) {
                    Some(uid) => *source = uid,
                    None => {
                        self.say(ctx, "たどる線がありません（線・スイープ・押し出しなどを先に作ります）");
                        return;
                    }
                }
            }
            Pattern::Geometry { source, .. } => {
                let found = self.scene.groups.iter().enumerate().find_map(|(gi, gr)| {
                    gr.shapes.iter().position(|s| pattern::shape_polyline(s).is_some()).or_else(|| (gi != g).then_some(0).filter(|_| !gr.shapes.is_empty())).map(|si| (gi, si))
                });
                match found.and_then(|(gi, si)| self.scene.ensure_shape_uid(gi, si)) {
                    Some(uid) => *source = Source::Shape(uid),
                    None => {
                        self.say(ctx, "点をかりる、ほかの形がありません");
                        return;
                    }
                }
            }
            _ => {}
        }
        if let Some(gr) = self.scene.groups.get_mut(g) {
            gr.pattern = Some(p);
        }
        self.sel = Some(Pick::Group(g));
    }

    /// Turn the mirror about world axis `axis` (0 X, 1 Y, 2 Z) on or off for the selected group.
    pub fn toggle_mirror(&mut self, ctx: &egui::Context, axis: usize) {
        let Some(g) = self.selected_group() else {
            self.say(ctx, "グループか形をえらんでください");
            return;
        };
        let Some(gr) = self.scene.groups.get_mut(g) else { return };
        match &mut gr.pattern {
            None => {
                let mut axes = [false; 3];
                if let Some(a) = axes.get_mut(axis) {
                    *a = true;
                }
                gr.pattern = Some(Pattern::Mirror { axes, centre: false });
            }
            Some(Pattern::Mirror { axes, .. }) => {
                if let Some(a) = axes.get_mut(axis) {
                    *a = !*a;
                }
                if !axes.iter().any(|a| *a) {
                    gr.pattern = None;
                }
            }
            Some(_) => {
                self.say(ctx, "ほかのパターンがついています。先にパターンをやめてください");
                return;
            }
        }
        self.sel = Some(Pick::Group(g));
    }

    /// Take the pattern off the selected group.
    pub fn clear_pattern(&mut self) {
        if let Some(g) = self.selected_group()
            && let Some(gr) = self.scene.groups.get_mut(g)
        {
            gr.pattern = None;
        }
    }

    /// A linked copy of the selected group: it shows the same shapes, and follows every edit of them.
    pub fn make_instance(&mut self, ctx: &egui::Context) {
        let Some(g) = self.selected_group() else {
            self.say(ctx, "グループか形をえらんでください");
            return;
        };
        if self.scene.groups.len() >= MAX_GROUPS {
            self.say(ctx, format!("グループは{MAX_GROUPS}個までです"));
            return;
        }
        // a copy of a copy is a copy of the original
        let src = pattern::link_source(&self.scene, g).unwrap_or(g);
        let Some(uid) = self.scene.ensure_group_uid(src) else { return };
        let (name, reach) = match self.scene.groups.get(src) {
            Some(s) => (s.name.clone(), pattern::group_sphere(&s.shapes).r),
            None => return,
        };
        let mut copy = Group::new(format!("{name} (リンク)"));
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [(reach * 2.0 + 10.0).clamp(20.0, 400.0), 0.0, 0.0], ..Xform::default() };
        let at = (g + 1).min(self.scene.groups.len());
        self.scene.groups.insert(at, copy);
        self.sel = Some(Pick::Group(at));
        self.say(ctx, "リンクコピーを作りました。元を直すと、こちらも変わります");
    }

    /// Make a linked copy a group of its own again (its shapes are copied, placed where the copy stood).
    fn unlink_group(&mut self, g: usize) {
        let Some(src) = pattern::link_source(&self.scene, g) else { return };
        let Some(x) = self.scene.groups.get(g).map(|gr| gr.xform) else { return };
        let Some(source) = self.scene.groups.get(src) else { return };
        let inst = x.inst(pattern::centre_of_shapes(&source.shapes));
        let mut shapes = source.shapes.clone();
        for s in &mut shapes {
            let r = crate::sdf::rotation(s.rot);
            let m = inst.m;
            let mut mr = [[0.0_f32; 3]; 3];
            for (a, row) in mr.iter_mut().enumerate() {
                for (b, v) in row.iter_mut().enumerate() {
                    *v = (0..3).map(|k| m[a][k] * r[k][b]).sum();
                }
            }
            s.pos = inst.apply(s.pos);
            s.rot = pattern::euler_of(&mr);
            for v in &mut s.scale {
                *v = (*v * inst.s).clamp(0.01, 100.0);
            }
            s.uid = 0;
        }
        if let Some(gr) = self.scene.groups.get_mut(g) {
            gr.shapes = shapes;
            gr.instance_of = None;
            gr.xform = Xform::default();
        }
    }

    /// The pattern part of the group inspector (and the linked copy's placement).
    pub fn pattern_section(&mut self, ui: &mut Ui, g: usize) {
        let ctx = ui.ctx().clone();
        if self.scene.groups.get(g).is_some_and(Group::is_linked) {
            self.link_box(ui, g);
        }
        style::title(ui, "パターン");
        let current = self.scene.groups.get(g).and_then(|gr| gr.pattern.clone());
        // the kind: nothing, or one of five
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if style::icon_button(ui, 34.0, "パターンなし", current.is_none(), |p, r, ink| {
                let f = frame(r);
                p.line_segment([at(f, 0.2, 0.2), at(f, 0.8, 0.8)], Stroke::new(2.0, ink));
                p.line_segment([at(f, 0.8, 0.2), at(f, 0.2, 0.8)], Stroke::new(2.0, ink));
            })
            .clicked()
            {
                self.clear_pattern();
            }
            for k in PatternKind::ALL {
                let on = current.as_ref().is_some_and(|p| p.kind() == k);
                if style::icon_button(ui, 34.0, &format!("{}　{}", k.label(), k.hint()), on, |p, r, ink| icon(k, p, r, ink)).clicked() && !on {
                    self.set_pattern_kind(&ctx, k);
                }
            }
        });
        // the quick mirror: X / Y / Z
        let mirror_axes = match &current {
            Some(Pattern::Mirror { axes, .. }) => Some(*axes),
            _ => None,
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            style::row_label(ui, "ミラー");
            let free = current.is_none() || mirror_axes.is_some();
            for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                let on = mirror_axes.is_some_and(|a| a[i]);
                if style::pill_enabled(ui, name, on, free)
                    .on_hover_text(["X（左右）に写す", "Y（前後）に写す", "Z（上下）に写す"][i])
                    .clicked()
                {
                    self.toggle_mirror(&ctx, i);
                }
            }
        });
        let Some(mut pat) = current.clone() else {
            ui.label(RichText::new("グループを、いくつもならべたり、左右に写したりできます").size(12.0).color(style::DIM));
            return;
        };
        let centre = self.scene.groups.get(g).map(|gr| pattern::centre_of_shapes(&gr.shapes)).unwrap_or([0.0; 3]);
        let mut chosen: Option<Chosen> = None;
        match &mut pat {
            Pattern::Mirror { centre: about, .. } => {
                ui.horizontal(|ui| {
                    style::row_label(ui, "写す面");
                    if style::pill(ui, "原点", !*about).on_hover_text("世界の真ん中（原点）を通る面").clicked() {
                        *about = false;
                    }
                    if style::pill(ui, "グループの中心", *about).on_hover_text("このグループの中心を通る面").clicked() {
                        *about = true;
                    }
                });
            }
            Pattern::Rect { counts, spacing } => {
                ui.horizontal(|ui| {
                    style::row_label(ui, "こすう");
                    for (i, c) in counts.iter_mut().enumerate() {
                        ui.label(RichText::new(["X", "Y", "Z"][i]).color(style::AXIS[i]).size(12.0).strong());
                        style::field_sized(ui, [44.0, 22.0], egui::DragValue::new(c).speed(0.1).range(1..=pattern::MAX_SIDE));
                    }
                });
                style::xyz(ui, "あいだ", spacing, 0.5, -5000.0..=5000.0, "");
            }
            Pattern::Circular { count, angle, axis, centre: c, rotate } => {
                ui.horizontal(|ui| {
                    style::row_label(ui, "こすう");
                    style::field_sized(ui, [54.0, 22.0], egui::DragValue::new(count).speed(0.1).range(1..=pattern::MAX_INSTANCES as u32));
                    ui.label(RichText::new("角度").color(style::DIM).size(12.0));
                    style::field_sized(ui, [64.0, 22.0], egui::DragValue::new(angle).speed(1.0).range(-3600.0..=3600.0).suffix("°"));
                });
                ui.horizontal(|ui| {
                    style::row_label(ui, "回る軸");
                    for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                        if style::pill(ui, name, usize::from(*axis) == i).clicked() {
                            *axis = i as u8;
                        }
                    }
                });
                style::xyz(ui, "中心", c, 0.5, -5000.0..=5000.0, "");
                ui.horizontal(|ui| {
                    ui.add_space(style::LABEL_W);
                    if style::pill(ui, "原点", false).clicked() {
                        *c = [0.0; 3];
                    }
                    if style::pill(ui, "グループの中心", false).clicked() {
                        *c = centre;
                    }
                });
                style::checkbox(ui, rotate, "コピーもいっしょに回す");
            }
            Pattern::Path { source, count, mode, step, align } => {
                let label = self.source_label(Source::Shape(*source));
                ui.horizontal(|ui| {
                    style::row_label(ui, "たどる線");
                    egui::ComboBox::from_id_salt("pattern_path_source").selected_text(label).width(150.0).show_ui(ui, |ui| {
                        for (gi, gr) in self.scene.groups.iter().enumerate() {
                            for (si, s) in gr.shapes.iter().enumerate() {
                                if pattern::shape_polyline(s).is_some() && ui.selectable_label(s.uid != 0 && s.uid == *source, format!("{} ({})", s.name, gr.name)).clicked() {
                                    chosen = Some(Chosen::Shape(gi, si));
                                }
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    style::row_label(ui, "ならべ方");
                    if style::pill(ui, "ぴったり", *mode == PathMode::Even).on_hover_text("線のはしからはしまで、同じ間かくで").clicked() {
                        *mode = PathMode::Even;
                    }
                    if style::pill(ui, "きょり", *mode == PathMode::Step).on_hover_text("決まった長さごとに").clicked() {
                        *mode = PathMode::Step;
                    }
                });
                ui.horizontal(|ui| {
                    style::row_label(ui, if *mode == PathMode::Even { "こすう" } else { "多くても" });
                    style::field_sized(ui, [54.0, 22.0], egui::DragValue::new(count).speed(0.1).range(1..=pattern::MAX_INSTANCES as u32));
                    if *mode == PathMode::Step {
                        ui.label(RichText::new("間かく").color(style::DIM).size(12.0));
                        style::field_sized(ui, [70.0, 22.0], egui::DragValue::new(step).speed(0.5).range(0.1..=5000.0).suffix(" mm"));
                    }
                });
                style::checkbox(ui, align, "線のむきにそろえる");
            }
            Pattern::Geometry { source, orient } => {
                let label = self.source_label(*source);
                ui.horizontal(|ui| {
                    style::row_label(ui, "点をかりる");
                    egui::ComboBox::from_id_salt("pattern_geometry_source").selected_text(label).width(150.0).show_ui(ui, |ui| {
                        for (gi, gr) in self.scene.groups.iter().enumerate() {
                            if !gr.shapes.is_empty() && ui.selectable_label(matches!(source, Source::Group(u) if gr.uid != 0 && *u == gr.uid), format!("グループ: {}", gr.name)).clicked() {
                                chosen = Some(Chosen::Group(gi));
                            }
                            for (si, s) in gr.shapes.iter().enumerate() {
                                if ui.selectable_label(matches!(source, Source::Shape(u) if s.uid != 0 && *u == s.uid), format!("{} ({})", s.name, gr.name)).clicked() {
                                    chosen = Some(Chosen::Shape(gi, si));
                                }
                            }
                        }
                    });
                });
                style::checkbox(ui, orient, "点のむきにそろえる");
                ui.label(RichText::new("線・スイープ・押し出しは ふし・かど、箱は 8つの角、ほかは中心に").size(11.5).color(style::DIM));
            }
        }
        // what was picked from a list: give that shape or group a number to be found by
        match chosen {
            Some(Chosen::Shape(gi, si)) => {
                if let Some(uid) = self.scene.ensure_shape_uid(gi, si) {
                    match &mut pat {
                        Pattern::Path { source, .. } => *source = uid,
                        Pattern::Geometry { source, .. } => *source = Source::Shape(uid),
                        _ => {}
                    }
                }
            }
            Some(Chosen::Group(gi)) => {
                if let Some(uid) = self.scene.ensure_group_uid(gi)
                    && let Pattern::Geometry { source, .. } = &mut pat
                {
                    *source = Source::Group(uid);
                }
            }
            None => {}
        }
        // how many it makes
        let (n, capped) = pattern::describe(&self.scene, g);
        let missing = pat.follows().is_some() && pattern::natural_count(&pat, &self.scene) == 0;
        ui.horizontal(|ui| {
            style::row_label(ui, "いまの数");
            ui.label(RichText::new(format!("{n} こ")).strong().size(13.0));
        });
        if capped {
            ui.label(RichText::new(pattern::TOO_MANY).size(12.0).color(style::CUT));
        }
        if missing {
            ui.label(RichText::new("たどるもの（線・形）が見つかりません").size(12.0).color(style::CUT));
        }
        if let Some(gr) = self.scene.groups.get_mut(g)
            && gr.pattern.as_ref() != Some(&pat)
        {
            gr.pattern = Some(pat);
        }
        ui.label(RichText::new("形のワクは使いません。コピーを選ぶと、元のグループがえらばれます").size(11.5).color(style::DIM));
    }

    fn source_label(&self, s: Source) -> String {
        match s {
            Source::Shape(uid) => match self.scene.shape_by_uid(uid).and_then(|(g, i)| self.scene.shape(g, i)) {
                Some(sh) => sh.name.clone(),
                None => "（えらんでください）".into(),
            },
            Source::Group(uid) => match self.scene.group_by_uid(uid).and_then(|g| self.scene.groups.get(g)) {
                Some(gr) => format!("グループ: {}", gr.name),
                None => "（えらんでください）".into(),
            },
        }
    }

    /// The box for a linked copy: whose shapes it shows, and where it stands.
    fn link_box(&mut self, ui: &mut Ui, g: usize) {
        style::title(ui, "リンクコピー");
        let src = pattern::link_source(&self.scene, g);
        let src_name = src.and_then(|s| self.scene.groups.get(s)).map(|s| s.name.clone());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("もと").color(style::DIM).size(12.0));
            match &src_name {
                Some(n) => ui.label(RichText::new(n).strong().size(13.0)),
                None => ui.label(RichText::new("元のグループがありません").color(style::CUT).size(12.0)),
            };
        });
        if let Some(gr) = self.scene.groups.get_mut(g) {
            style::xyz(ui, "位置", &mut gr.xform.pos, 0.5, -5000.0..=5000.0, "");
            style::xyz(ui, "回転", &mut gr.xform.rot, 1.0, -3600.0..=3600.0, "°");
            ui.horizontal(|ui| {
                style::row_label(ui, "大きさ");
                style::field_sized(ui, [74.0, 22.0], egui::DragValue::new(&mut gr.xform.scale).speed(0.01).range(0.01..=100.0).suffix("×").max_decimals(2));
            });
        }
        ui.horizontal(|ui| {
            if style::pill_enabled(ui, "元をえらぶ", false, src.is_some()).clicked()
                && let Some(s) = src
            {
                self.sel = Some(Pick::Group(s));
            }
            if style::pill_enabled(ui, "リンクをはずす", false, src.is_some()).on_hover_text("この組の形として、コピーします（もう元とつながりません）").clicked() {
                self.unlink_group(g);
            }
        });
        ui.label(RichText::new("元を直すと、リンクコピーも同じに変わります").size(11.5).color(style::DIM));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material, Shape};

    fn app_with_two_groups() -> App {
        let mut app = App::new(true, None);
        let mut g = Group::new("もう一つ");
        g.shapes.push(Shape::new(Kind::Box, [40.0, 0.0, 10.0], Material::default()));
        app.scene.groups.push(g);
        app
    }

    #[test]
    fn the_mirror_toggles_make_and_remove_the_pattern() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        app.toggle_mirror(&ctx, 0);
        assert!(matches!(app.scene.groups[0].pattern, Some(Pattern::Mirror { axes: [true, false, false], .. })));
        assert_eq!(app.sel, Some(Pick::Group(0)));
        app.toggle_mirror(&ctx, 2);
        assert!(matches!(app.scene.groups[0].pattern, Some(Pattern::Mirror { axes: [true, false, true], .. })));
        assert_eq!(pattern::describe(&app.scene, 0), (4, false));
        app.toggle_mirror(&ctx, 0);
        app.toggle_mirror(&ctx, 2);
        assert!(app.scene.groups[0].pattern.is_none(), "no axis left: no pattern");
        // another pattern is not overwritten by a mirror toggle
        app.set_pattern_kind(&ctx, PatternKind::Rect);
        app.toggle_mirror(&ctx, 1);
        assert!(matches!(app.scene.groups[0].pattern, Some(Pattern::Rect { .. })));
        app.clear_pattern();
        assert!(app.scene.groups[0].pattern.is_none());
        // nothing selected: nothing happens
        app.sel = None;
        app.toggle_mirror(&ctx, 0);
        app.set_pattern_kind(&ctx, PatternKind::Circular);
        assert!(app.scene.groups[0].pattern.is_none());
    }

    #[test]
    fn path_and_geometry_patterns_find_something_to_follow() {
        let ctx = egui::Context::default();
        let mut app = app_with_two_groups();
        app.sel = Some(Pick::Group(0));
        // no line in the scene yet
        app.set_pattern_kind(&ctx, PatternKind::Path);
        assert!(app.scene.groups[0].pattern.is_none(), "nothing to follow");
        let mut line = Shape::new(Kind::Curve, [0.0, 0.0, 0.0], Material::default());
        line.path = vec![[0.0, 0.0, 0.0, 3.0], [60.0, 40.0, 0.0, 3.0], [120.0, 0.0, 0.0, 3.0]];
        app.scene.groups[1].shapes.push(line);
        app.set_pattern_kind(&ctx, PatternKind::Path);
        let Some(Pattern::Path { source, .. }) = app.scene.groups[0].pattern.clone() else { panic!("a path pattern") };
        assert_eq!(app.scene.shape_by_uid(source), Some((1, 1)));
        assert_eq!(pattern::describe(&app.scene, 0), (8, false));
        app.set_pattern_kind(&ctx, PatternKind::Geometry);
        assert!(matches!(app.scene.groups[0].pattern, Some(Pattern::Geometry { .. })));
        assert!(pattern::describe(&app.scene, 0).0 >= 1);
    }

    #[test]
    fn an_instance_is_a_linked_copy_that_follows_the_source() {
        let ctx = egui::Context::default();
        let mut app = app_with_two_groups();
        app.sel = Some(Pick::Shape(1, 0));
        app.make_instance(&ctx);
        assert_eq!(app.scene.groups.len(), 3);
        assert_eq!(app.sel, Some(Pick::Group(2)));
        let copy = &app.scene.groups[2];
        assert!(copy.is_linked() && copy.shapes.is_empty() && copy.xform.pos[0] > 10.0);
        assert_eq!(app.scene.group_by_uid(copy.instance_of.unwrap_or(0)), Some(1));
        let before = crate::sdf::Field::new(&app.scene).distance([40.0 + copy.xform.pos[0], 0.0, 10.0]);
        assert!(before < 0.0, "the copy is a box where it stands");
        // a copy of the copy is a copy of the original
        app.make_instance(&ctx);
        assert_eq!(app.scene.groups.len(), 4);
        assert_eq!(app.scene.groups[3].instance_of, app.scene.groups[2].instance_of);
        // editing the source shows in the copies
        app.scene.groups[1].shapes[0].size = [4.0, 4.0, 4.0];
        let after = crate::sdf::Field::new(&app.scene).distance([40.0 + app.scene.groups[2].xform.pos[0], 0.0, 10.0]);
        assert!(after < 0.0 && after > before, "{after} {before}");
        // unlinking makes a group of its own, where the copy stood
        app.unlink_group(2);
        let g = &app.scene.groups[2];
        assert!(!g.is_linked() && g.shapes.len() == 1);
        assert!(g.shapes[0].pos[0] > 40.0 + 10.0, "it stands where the copy stood, beside the source");
        assert!(crate::sdf::Field::new(&app.scene).distance([g.shapes[0].pos[0], 0.0, 10.0]) < 0.0);
        // the limit
        while app.scene.groups.len() < MAX_GROUPS {
            app.scene.groups.push(Group::new("x"));
        }
        let n = app.scene.groups.len();
        app.make_instance(&ctx);
        assert_eq!(app.scene.groups.len(), n);
    }

    #[test]
    fn the_inspector_draws_every_pattern_and_the_linked_copy_box() {
        let ctx = egui::Context::default();
        let mut app = app_with_two_groups();
        let mut line = Shape::new(Kind::Sweep, [0.0; 3], Material::default());
        line.path = vec![[0.0, 0.0, 0.0, 1.0], [60.0, 0.0, 0.0, 1.0]];
        app.scene.groups[1].shapes.push(line);
        app.sel = Some(Pick::Group(0));
        for kind in PatternKind::ALL {
            app.set_pattern_kind(&ctx, kind);
            assert!(app.scene.groups[0].pattern.is_some(), "{kind:?}");
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.pattern_section(ui, 0));
            });
        }
        app.clear_pattern();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.pattern_section(ui, 0));
        });
        app.sel = Some(Pick::Group(1));
        app.make_instance(&ctx);
        let Some(Pick::Group(c)) = app.sel else { panic!("the copy is selected") };
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.pattern_section(ui, c));
        });
        // a pattern that cannot find its line says so and still draws
        app.scene.groups[0].pattern = Some(Pattern::Path { source: 4242, count: 3, mode: PathMode::Even, step: 5.0, align: true });
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.pattern_section(ui, 0));
        });
    }

    #[test]
    fn every_pattern_icon_draws() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for k in PatternKind::ALL {
                icon(k, ui.painter(), Rect::from_min_size(pos2(0.0, 0.0), vec2(34.0, 34.0)), Color32::WHITE);
            }
        });
    }
}
