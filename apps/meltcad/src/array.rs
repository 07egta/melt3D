//! Arrays: many copies of the selected shape or group in a line or around the Z axis.
//! The small card keeps its numbers in egui's memory, like the SVG card.

use egui::{Align2, RichText};

use crate::app::App;
use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Pick, Shape};
use crate::pattern::Xform;
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

/// Is the array card showing? (Keys such as Delete must not reach the scene behind it.)
pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<Dlg>(id())).is_some()
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|m| m.insert_temp(id(), Dlg::default()));
}

/// How far (degrees) copy number `k` is turned about Z. A full turn is divided evenly (no copy on
/// top of the original); a part turn ends on its last copy.
fn turn(d: &Dlg, k: u32) -> f32 {
    let per = if d.angle >= 359.5 { d.angle / d.count as f32 } else { d.angle / (d.count.saturating_sub(1)).max(1) as f32 };
    per * k as f32
}

/// Where copy number `k` of a linked copy goes: the same move or turn, applied to its transform.
/// `c` is the middle of the group it shows (the transform turns about it).
fn place_xform(x: &mut Xform, c: [f32; 3], d: &Dlg, k: u32) {
    match d.mode {
        Mode::Line => {
            for i in 0..3 {
                x.pos[i] += d.step[i] * k as f32;
            }
        }
        Mode::Circle => {
            let a = turn(d, k);
            let (sn, cs) = a.to_radians().sin_cos();
            // the point the copy now stands at, turned about the Z axis, minus where it was
            let (wx, wy) = (c[0] + x.pos[0], c[1] + x.pos[1]);
            x.pos[0] = wx * cs - wy * sn - c[0];
            x.pos[1] = wx * sn + wy * cs - c[1];
            x.rot[2] += a;
        }
    }
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
            let deg = turn(d, k);
            let (sn, cs) = deg.to_radians().sin_cos();
            let (x, y) = (s.pos[0], s.pos[1]);
            s.pos[0] = x * cs - y * sn;
            s.pos[1] = x * sn + y * cs;
            s.rot[2] += deg;
        }
    }
}

impl App {
    /// Make `d.count - 1` more copies of the selection. Returns how many were made.
    fn make_array(&mut self, d: &Dlg) -> Result<usize, String> {
        if !self.allow_scene_structure_change() {
            return Err(self.sk.msg.clone());
        }
        let copies = d.count.clamp(1, 64) - 1;
        if copies == 0 {
            return Err("Please use 2 or more".into());
        }
        match self.sel {
            Some(Pick::Shape(g, s)) => {
                let base = self.scene.shape(g, s).cloned().ok_or("No shape")?;
                if self.scene.shape_count() + copies as usize > MAX_SHAPES {
                    return Err(format!("Up to {MAX_SHAPES} shapes"));
                }
                let more: Vec<Shape> = (0..copies).map(|_| base.clone()).collect();
                crate::mesh::can_add(&self.scene, more.iter())?;
                for k in 1..=copies {
                    let mut c = base.clone();
                    place(&mut c, d, k);
                    // An array copy is a new animation/pattern identity, not another reference to
                    // its source: sharing the uid let the copy's placement overwrite the source's
                    // pose at the frame the array was made in.
                    c.uid = 0;
                    c.name = format!("{} {}", base.name, k + 1);
                    self.scene.groups[g].shapes.insert(s + k as usize, c);
                }
                self.sel = Some(Pick::Shape(g, s + copies as usize));
                Ok(copies as usize)
            }
            Some(Pick::Group(g)) => {
                // the whole outline under the group: its child groups come along
                let end = self.scene.subtree_end(g);
                let tree: Vec<Group> = self.scene.groups.get(g..end).ok_or("No group")?.to_vec();
                let n = tree.len();
                let shapes: usize = tree.iter().map(|t| t.shapes.len()).sum();
                if self.scene.groups.len() + n * copies as usize > MAX_GROUPS || self.scene.shape_count() + shapes * copies as usize > MAX_SHAPES {
                    return Err("That goes over the limit".into());
                }
                crate::mesh::can_add(&self.scene, (0..copies).flat_map(|_| tree.iter().flat_map(|t| t.shapes.iter())))?;
                // a linked copy turns about the middle of the group it shows
                let centres: Vec<[f32; 3]> = tree
                    .iter()
                    .map(|t| {
                        t.instance_of
                            .and_then(|u| self.scene.group_by_uid(u))
                            .and_then(|i| self.scene.groups.get(i))
                            .map(|src| crate::pattern::centre_of_shapes(&src.shapes))
                            .unwrap_or([0.0; 3])
                    })
                    .collect();
                let mut next = self.scene.next_uid();
                for k in 1..=copies {
                    let mut copy = tree.clone();
                    // each copy is a set of its own: links and patterns inside it follow it
                    crate::model::Scene::renumber_copies(&mut next, &mut copy);
                    for (c, centre) in copy.iter_mut().zip(&centres) {
                        for s in &mut c.shapes {
                            place(s, d, k);
                        }
                        if c.instance_of.is_some() {
                            place_xform(&mut c.xform, *centre, d, k);
                        }
                    }
                    if let Some(top) = copy.first_mut() {
                        top.name = format!("{} {}", tree[0].name, k + 1);
                    }
                    let at = end + (k as usize - 1) * n;
                    self.scene.groups.splice(at..at, copy);
                }
                self.scene.dedupe_uids();
                self.sel = Some(Pick::Group(end + (copies as usize - 1) * n));
                Ok(copies as usize)
            }
            None => Err("Select something to arrange".into()),
        }
    }
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|m| m.get_temp::<Dlg>(id())) else { return };
    let (mut go, mut close) = (false, false);
    egui::Area::new(egui::Id::new("array_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(300.0);
            ui.label(RichText::new("Array copy").strong().size(15.0));
            ui.label(RichText::new("Repeats the selected shape (group) many times").size(11.5).color(style::DIM));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if style::pill(ui, "Straight", d.mode == Mode::Line).clicked() {
                    d.mode = Mode::Line;
                }
                if style::pill(ui, "Circle (around Z)", d.mode == Mode::Circle).clicked() {
                    d.mode = Mode::Circle;
                }
            });
            egui::Grid::new("array_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("Total").color(style::DIM));
                ui.add(egui::DragValue::new(&mut d.count).range(2..=64).suffix(" pcs"));
                ui.end_row();
                match d.mode {
                    Mode::Line => {
                        for (i, n) in ["Width X", "Depth Y", "Height Z"].iter().enumerate() {
                            ui.label(RichText::new(*n).color(style::DIM));
                            ui.add(egui::DragValue::new(&mut d.step[i]).speed(0.5).range(-1000.0..=1000.0).suffix(" mm").max_decimals(1));
                            ui.end_row();
                        }
                    }
                    Mode::Circle => {
                        ui.label(RichText::new("Angle").color(style::DIM));
                        ui.add(egui::DragValue::new(&mut d.angle).speed(1.0).range(10.0..=360.0).suffix(" °"));
                        ui.end_row();
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !app.scene_structure_locked(),
                        egui::Button::new(RichText::new("Array").strong().color(egui::Color32::WHITE))
                            .fill(style::ACCENT_SOFT)
                            .stroke(egui::Stroke::new(1.0, style::ACCENT)),
                    )
                    .clicked()
                {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    });
    // Esc closes the card (the keys behind it are paused while it shows)
    if !ctx.egui_wants_keyboard_input() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        close = true;
    }
    if go {
        match app.make_array(&d) {
            Ok(n) => {
                app.say(ctx, format!("Added {n} copies"));
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
    fn an_arrayed_shape_keeps_the_sources_pose_and_gives_copies_their_own_identity() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups[0].shapes[0].pos = [0.0, 0.0, 20.0];
        let mut anim = std::mem::take(&mut app.scene.anim);
        anim.capture(&mut app.scene, 0).expect("pose 0");
        app.scene.anim = anim;
        let uid = app.scene.groups[0].shapes[0].uid;
        assert_ne!(uid, 0, "the source has an animation identity");

        app.sel = Some(Pick::Shape(0, 0));
        assert_eq!(app.make_array(&Dlg { count: 4, ..Dlg::default() }), Ok(3));
        // the next UI frame records the changed scene as a pose (before `settle` drops repeated uids)
        app.an.open = true;
        app.an.frame = 0;
        app.animation_capture_finished(&ctx);
        app.settle(&ctx);

        let pose = app.scene.anim.tracks.iter().find(|t| t.uid == uid).expect("the source's track");
        assert_eq!(pose.keys.iter().find(|k| k.0 == 0).map(|k| k.1.pos), Some([0.0, 0.0, 20.0]), "frame 0 still holds the source, not a copy");
        assert!(app.scene.groups[0].shapes[1..].iter().all(|s| s.uid != uid), "no copy claims the source's identity");
    }

    #[test]
    fn the_card_draws() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
    }

    fn nested_scene(app: &mut App) {
        use crate::model::Group;
        let mut child = Group::new("child");
        child.depth = 1;
        let mut shape = app.scene.groups[0].shapes[0].clone();
        shape.pos = [0.0, 0.0, 5.0];
        child.shapes.push(shape);
        app.scene.groups.insert(1, child);
    }

    #[test]
    fn arraying_a_group_copies_its_child_groups_too() {
        let mut app = App::new(true, None);
        nested_scene(&mut app);
        let before = app.scene.groups.len();
        let shapes = app.scene.shape_count();
        app.sel = Some(Pick::Group(0));
        assert_eq!(app.make_array(&Dlg { count: 3, ..Dlg::default() }), Ok(2));
        assert_eq!(app.scene.groups.len(), before + 4);
        assert_eq!(app.scene.shape_count(), shapes * 3);
        let depths: Vec<u8> = app.scene.groups.iter().map(|g| g.depth).collect();
        assert_eq!(&depths[..6], &[0, 1, 0, 1, 0, 1][..depths.len().min(6)]);
        assert_eq!(app.scene.groups[3].shapes[0].pos[0], 30.0, "the child moved with its copy");
        assert_eq!(app.sel, Some(Pick::Group(4)));
    }

    #[test]
    fn linked_copies_array_apart_instead_of_piling_up() {
        let mut app = App::new(true, None);
        let uid = app.scene.ensure_group_uid(0).unwrap();
        let mut link = Group::new("link");
        link.instance_of = Some(uid);
        app.scene.groups.push(link);
        let at = app.scene.groups.len() - 1;
        app.sel = Some(Pick::Group(at));
        assert_eq!(app.make_array(&Dlg { count: 3, ..Dlg::default() }), Ok(2));
        let xs: Vec<f32> = app.scene.groups[at..].iter().map(|g| g.xform.pos[0]).collect();
        assert_eq!(xs, vec![0.0, 30.0, 60.0]);
        // around the Z axis the copies turn about the Z axis, not all on the spot
        app.scene.groups[0].shapes[0].pos = [20.0, 0.0, 5.0];
        let at2 = at;
        app.sel = Some(Pick::Group(at2));
        let d = Dlg { mode: Mode::Circle, count: 4, angle: 360.0, ..Dlg::default() };
        assert_eq!(app.make_array(&d), Ok(3));
        let c = crate::pattern::centre_of_shapes(&app.scene.groups[0].shapes);
        let g1 = &app.scene.groups[at2 + 1].xform;
        assert!((c[0] + g1.pos[0]).abs() < 1e-3 && (c[1] + g1.pos[1] - 20.0).abs() < 1e-3, "{g1:?}");
    }

    #[test]
    fn the_card_keeps_the_delete_key_from_the_scene_behind_it() {
        let ctx = egui::Context::default();
        assert!(!is_open(&ctx));
        open(&ctx);
        assert!(is_open(&ctx));
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        let n = app.scene.shape_count();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.keys(ui.ctx())).textures_delta.clear();
        assert_eq!(app.scene.shape_count(), n);
    }
}
