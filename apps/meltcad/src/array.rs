//! Arrays: many copies of the selected shape or group in a line or around the Z axis.
//! The small card keeps its numbers in egui's memory, like the SVG card.

use egui::{Align2, RichText};

use crate::app::App;
use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Pick, Shape};
use crate::style;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Line,
    Circle,
}

#[derive(Clone)]
struct Dlg {
    mode: Mode,
    count: u32,
    step: [f32; 3],
    /// Circle: the angle all the copies spread over (360 = all the way around).
    angle: f32,
}

impl Default for Dlg {
    fn default() -> Dlg {
        Dlg { mode: Mode::Line, count: 4, step: [30.0, 0.0, 0.0], angle: 360.0 }
    }
}

fn id() -> egui::Id {
    egui::Id::new("array_card")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), Dlg::default()));
}

/// Where copy number `k` (1-based) of a shape goes. Line: moved by k steps. Circle: turned about the Z axis.
fn place(s: &mut Shape, d: &Dlg, k: u32) {
    match d.mode {
        Mode::Line => {
            for i in 0..3 {
                s.pos[i] += d.step[i] * k as f32;
            }
        }
        Mode::Circle => {
            // a full turn is divided evenly (no copy on top of the original); a part turn ends on its last copy
            let per = if d.angle >= 359.5 { d.angle / d.count as f32 } else { d.angle / (d.count.saturating_sub(1)).max(1) as f32 };
            let a = (per * k as f32).to_radians();
            let (sn, cs) = a.sin_cos();
            let (x, y) = (s.pos[0], s.pos[1]);
            s.pos[0] = x * cs - y * sn;
            s.pos[1] = x * sn + y * cs;
            s.rot[2] += per * k as f32;
        }
    }
}

impl App {
    /// Make `d.count - 1` more copies of the selection. Returns how many were made.
    fn make_array(&mut self, d: &Dlg) -> Result<usize, String> {
        let copies = d.count.clamp(1, 64) - 1;
        if copies == 0 {
            return Err("2こ以上にしてください".into());
        }
        match self.sel {
            Some(Pick::Shape(g, s)) => {
                let base = self.scene.shape(g, s).cloned().ok_or("形がありません")?;
                if self.scene.shape_count() + copies as usize > MAX_SHAPES {
                    return Err(format!("形は{MAX_SHAPES}個までです"));
                }
                let more: Vec<Shape> = (0..copies).map(|_| base.clone()).collect();
                crate::mesh::can_add(&self.scene, more.iter())?;
                for k in 1..=copies {
                    let mut c = base.clone();
                    place(&mut c, d, k);
                    c.name = format!("{} {}", base.name, k + 1);
                    self.scene.groups[g].shapes.insert(s + k as usize, c);
                }
                self.sel = Some(Pick::Shape(g, s + copies as usize));
                Ok(copies as usize)
            }
            Some(Pick::Group(g)) => {
                let base: Group = self.scene.groups.get(g).cloned().ok_or("グループがありません")?;
                if self.scene.groups.len() + copies as usize > MAX_GROUPS
                    || self.scene.shape_count() + base.shapes.len() * copies as usize > MAX_SHAPES
                {
                    return Err("ふやせる数をこえます".into());
                }
                crate::mesh::can_add(&self.scene, (0..copies).flat_map(|_| base.shapes.iter()))?;
                let end = self.scene.subtree_end(g);
                for k in 1..=copies {
                    let mut c = base.clone();
                    for s in &mut c.shapes {
                        place(s, d, k);
                    }
                    c.name = format!("{} {}", base.name, k + 1);
                    self.scene.groups.insert(end + k as usize - 1, c);
                }
                self.sel = Some(Pick::Group(end + copies as usize - 1));
                Ok(copies as usize)
            }
            None => Err("ならべるものをえらんでください".into()),
        }
    }
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|m| m.get_temp::<Dlg>(id())) else { return };
    let (mut go, mut close) = (false, false);
    egui::Area::new(egui::Id::new("array_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(300.0);
            ui.label(RichText::new("ならべてコピー").strong().size(15.0));
            ui.label(RichText::new("えらんでいる形（グループ）を、いくつもならべます").size(11.5).color(style::DIM));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if style::pill(ui, "まっすぐ", d.mode == Mode::Line).clicked() {
                    d.mode = Mode::Line;
                }
                if style::pill(ui, "ぐるっと（Z軸のまわり）", d.mode == Mode::Circle).clicked() {
                    d.mode = Mode::Circle;
                }
            });
            egui::Grid::new("array_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("ぜんぶで").color(style::DIM));
                ui.add(egui::DragValue::new(&mut d.count).range(2..=64).suffix(" こ"));
                ui.end_row();
                match d.mode {
                    Mode::Line => {
                        for (i, n) in ["よこ X", "おく Y", "たて Z"].iter().enumerate() {
                            ui.label(RichText::new(*n).color(style::DIM));
                            ui.add(egui::DragValue::new(&mut d.step[i]).speed(0.5).range(-1000.0..=1000.0).suffix(" mm").max_decimals(1));
                            ui.end_row();
                        }
                    }
                    Mode::Circle => {
                        ui.label(RichText::new("角度").color(style::DIM));
                        ui.add(egui::DragValue::new(&mut d.angle).speed(1.0).range(10.0..=360.0).suffix(" °"));
                        ui.end_row();
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("ならべる").strong().color(egui::Color32::WHITE))
                            .fill(style::ACCENT_SOFT)
                            .stroke(egui::Stroke::new(1.0, style::ACCENT)),
                    )
                    .clicked()
                {
                    go = true;
                }
                if ui.button("やめる").clicked() {
                    close = true;
                }
            });
        });
    });
    if go {
        match app.make_array(&d) {
            Ok(n) => {
                app.say(ctx, format!("{n}こふやしました"));
                close = true;
            }
            Err(e) => app.say(ctx, e),
        }
    }
    if close {
        ctx.data_mut(|m| m.remove::<Dlg>(id()));
    } else {
        ctx.data_mut(|m| m.insert_temp(id(), d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_array_steps_along_x() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        let d = Dlg { count: 4, ..Dlg::default() };
        assert_eq!(app.make_array(&d), Ok(3));
        let xs: Vec<f32> = app.scene.groups[0].shapes.iter().map(|s| s.pos[0]).collect();
        assert_eq!(xs, vec![0.0, 30.0, 60.0, 90.0]);
        assert_eq!(app.sel, Some(Pick::Shape(0, 3)));
    }

    #[test]
    fn a_circle_array_goes_round_and_turns_each_copy() {
        let mut app = App::new(true, None);
        app.scene.groups[0].shapes[0].pos = [20.0, 0.0, 5.0];
        app.sel = Some(Pick::Shape(0, 0));
        let d = Dlg { mode: Mode::Circle, count: 4, angle: 360.0, ..Dlg::default() };
        assert_eq!(app.make_array(&d), Ok(3));
        let s = &app.scene.groups[0].shapes;
        assert!((s[1].pos[0]).abs() < 1e-3 && (s[1].pos[1] - 20.0).abs() < 1e-3);
        assert!((s[2].pos[0] + 20.0).abs() < 1e-3);
        assert_eq!(s[3].rot[2], 270.0);
        assert!(s.iter().all(|x| x.pos[2] == 5.0));
    }

    #[test]
    fn groups_are_arrayed_too_and_the_limits_hold() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Group(0));
        assert_eq!(app.make_array(&Dlg { count: 3, ..Dlg::default() }), Ok(2));
        assert_eq!(app.scene.groups.len(), 3);
        // one array makes at most 63 copies; keep going and the group limit stops it
        let mut more = 0;
        while app.make_array(&Dlg { count: 64, ..Dlg::default() }).is_ok() {
            more += 1;
            assert!(more < 20, "the limit must stop the arrays");
        }
        assert!(app.scene.groups.len() > MAX_GROUPS - 63 && app.scene.groups.len() <= MAX_GROUPS);
        app.sel = None;
        assert!(app.make_array(&Dlg::default()).is_err());
        assert!(app.scene.groups.len() <= MAX_GROUPS && app.scene.shape_count() <= MAX_SHAPES);
    }

    #[test]
    fn the_card_draws() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
    }
}
