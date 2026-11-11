//! Chess Set preset card: six actual editable SDF pieces, three styles, and full board layout.
//! No meshes are baked; the normal scene inspector, duplicate, Undo and exporter keep working.

use egui::{Align2, Color32, RichText, Stroke};

use crate::app::App;
use crate::chess::{self, Design, Piece, Settings, Side};
use crate::model::{Pick, Scene};
use crate::style;

#[derive(Clone, Default)]
struct Dlg {
    settings: Settings,
}
#[derive(Clone, Copy)]
enum Action {
    AddOne,
    ReplaceSelected,
    FullSet,
}
fn id() -> egui::Id {
    egui::Id::new("chess_set_dialog")
}

fn selected_group(app: &App) -> Option<usize> {
    match app.sel {
        Some(Pick::Group(g)) | Some(Pick::Shape(g, _)) => Some(g),
        None => None,
    }
}

pub fn open(ctx: &egui::Context, app: &App) {
    let mut d = Dlg::default();
    if let Some(g) = selected_group(app).and_then(|i| app.scene.groups.get(i))
        && let Some(previous) = chess::preset_of(g)
    {
        d.settings = previous;
    }
    ctx.data_mut(|m| m.insert_temp(id(), d));
}

fn do_action(app: &mut App, ctx: &egui::Context, s: Settings, action: Action) -> Result<(), String> {
    let s = s.safe();
    match action {
        Action::AddOne => {
            let (c, r) = crate::gpu::bounds(&app.scene, None);
            let at = if r > 0.0 { [c[0] + r + s.diameter_mm * 0.5 + 8.0, c[1]] } else { [app.cam.target[0], app.cam.target[1]] };
            let g = chess::insert_piece(&mut app.scene, s, at)?;
            app.sel = Some(Pick::Group(g));
            app.frame_view(false);
            app.say(ctx, crate::i18n::fill("chess_dialog.added_piece", &[&s.piece.name(), &s.design.name()]));
        }
        Action::ReplaceSelected => {
            let g = selected_group(app).ok_or(crate::i18n::tr("chess_dialog.select_piece_rebuild"))?;
            chess::rebuild_piece(&mut app.scene, g, s)?;
            app.sel = Some(Pick::Group(g));
            app.frame_view(false);
            app.say(ctx, crate::i18n::fill("chess_dialog.rebuilt_selected", &[&s.design.name()]));
        }
        Action::FullSet => {
            let at = if app.scene == Scene::default() {
                [0.0, 0.0]
            } else {
                let (c, r) = crate::gpu::bounds(&app.scene, None);
                [c[0] + r + chess::board_width(s) * 0.5 + 10.0, c[1]]
            };
            let first = chess::insert_set(&mut app.scene, s, at)?;
            app.sel = Some(Pick::Group(first + 1)); // select the first piece, not the board
            app.frame_view(false);
            app.say(ctx, crate::i18n::tr("chess_dialog.board_placed"));
        }
    }
    Ok(())
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = ctx.data(|m| m.get_temp::<Dlg>(id())) else { return };
    let (mut close, mut action) = (false, None);
    let can_replace = selected_group(app).and_then(|i| app.scene.groups.get(i)).and_then(chess::preset_of).is_some();

    egui::Area::new(egui::Id::new("chess_set_area")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(352.0);
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.title")).strong().size(16.0));
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.intro")).size(11.5).color(style::DIM));
            ui.add_space(5.0);
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.piece_type_label")).color(style::DIM));
            ui.horizontal_wrapped(|ui| {
                for piece in Piece::ALL {
                    if style::pill(ui, piece.name(), d.settings.piece == piece).clicked() {
                        d.settings.piece = piece;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.design_label")).color(style::DIM));
            ui.horizontal_wrapped(|ui| {
                for design in Design::ALL {
                    if style::pill(ui, design.name(), d.settings.design == design).clicked() {
                        d.settings.design = design;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.piece_color_label")).color(style::DIM));
            ui.horizontal(|ui| {
                for side in Side::ALL {
                    if style::pill(ui, side.name(), d.settings.side == side).clicked() {
                        d.settings.side = side;
                    }
                }
            });
            ui.add_space(5.0);
            egui::Grid::new("chess_set_values").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label(crate::i18n::tr("chess_dialog.height_label"));
                ui.add(egui::DragValue::new(&mut d.settings.height_mm).range(24.0..=110.0).speed(0.5).suffix(" mm").max_decimals(1))
                    .on_hover_text(crate::i18n::tr("chess_dialog.height_hover"));
                ui.end_row();
                ui.label(crate::i18n::tr("chess_dialog.base_diameter_label"));
                ui.add(egui::DragValue::new(&mut d.settings.diameter_mm).range(14.0..=48.0).speed(0.25).suffix(" mm").max_decimals(1));
                ui.end_row();
                ui.label(crate::i18n::tr("chess_dialog.decoration_label"));
                ui.add(egui::Slider::new(&mut d.settings.ornament, 0.0..=1.0).show_value(true));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::fill("chess_dialog.board_width_note", &[&format!("{:.0}", chess::board_width(d.settings))]))
                    .size(11.0)
                    .color(style::DIM),
            );
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.group_note")).size(11.0).color(style::DIM));
            ui.label(RichText::new(crate::i18n::tr("chess_dialog.print_note")).size(11.0).color(style::DIM));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let create = egui::Button::new(RichText::new(crate::i18n::tr("chess_dialog.add_one_piece")).strong().color(Color32::WHITE))
                    .fill(style::ACCENT_SOFT)
                    .stroke(Stroke::new(1.0, style::ACCENT));
                if ui.add(create).clicked() {
                    action = Some(Action::AddOne);
                }
                if ui
                    .add_enabled(can_replace, egui::Button::new(crate::i18n::tr("chess_dialog.reedit_selected")))
                    .on_hover_text(crate::i18n::tr("chess_dialog.reedit_hover"))
                    .clicked()
                {
                    action = Some(Action::ReplaceSelected);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::tr("chess_dialog.place_board_and_pieces"))
                    .on_hover_text(crate::i18n::tr("chess_dialog.place_hover"))
                    .clicked()
                {
                    action = Some(Action::FullSet);
                }
                if ui.button(crate::i18n::tr("chess_dialog.close")).clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(chosen) = action {
        match do_action(app, ctx, d.settings, chosen) {
            Ok(()) => close = true,
            Err(reason) => app.say(ctx, reason),
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
    use crate::model::{Kind, Op};

    #[test]
    fn dialog_opens_and_imports_one_then_full_board() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        open(&ctx, &app);
        ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx())).textures_delta.clear();
        let one = Settings { piece: Piece::Knight, design: Design::Organic, ..Settings::default() };
        do_action(&mut app, &ctx, one, Action::AddOne).unwrap();
        let selected = selected_group(&app).unwrap();
        assert_eq!(chess::preset_of(&app.scene.groups[selected]), Some(one));
        assert!(app.scene.groups[selected].shapes.iter().any(|s| s.kind == Kind::Extrude));
        do_action(&mut app, &ctx, one, Action::ReplaceSelected).unwrap();
        assert_eq!(chess::preset_of(&app.scene.groups[selected]), Some(one));
        let before = app.scene.groups.len();
        do_action(&mut app, &ctx, Settings::default(), Action::FullSet).unwrap();
        assert_eq!(app.scene.groups.len(), before + 33);
        assert!(app.scene.groups.iter().all(|g| g.op == Op::Add));
    }

    #[test]
    fn individual_chess_piece_duplicates_saves_and_undoes() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        chess::insert_set(&mut app.scene, Settings::default(), [0.0, 0.0]).unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| app.settle(ui.ctx())).textures_delta.clear();
        assert_eq!(app.scene.groups.len(), 33);
        app.sel = Some(Pick::Group(1));
        let original = app.scene.groups[1].shapes[0].pos;
        app.duplicate();
        assert_eq!(app.scene.groups.len(), 34);
        let copied = &app.scene.groups[2];
        assert!(chess::preset_of(copied).is_some());
        // a duplicate stands where the original does (the app's duplicate no longer offsets it)
        assert_eq!(copied.shapes[0].pos[0], original[0]);
        ctx.run_ui(egui::RawInput::default(), |ui| app.settle(ui.ctx())).textures_delta.clear();
        app.undo();
        assert_eq!(app.scene.groups.len(), 33);
        app.redo();
        assert_eq!(app.scene.groups.len(), 34);
        assert_eq!(Scene::from_json(&app.scene.to_json()).unwrap(), app.scene);
    }

    #[test]
    fn rebuilding_does_not_modify_non_chess_groups() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let orig = app.scene.clone();
        assert!(do_action(&mut app, &ctx, Settings::default(), Action::ReplaceSelected).is_err());
        assert_eq!(app.scene, orig);
    }
}
