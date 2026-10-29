//! Puffy Text parameter card. Unlike a font renderer, this UI does NOT regenerate
//! the text on every frame: a single explicit apply becomes one undoable scene edit.

use egui::{Align2, Color32, RichText, Stroke};

use crate::app::App;
use crate::puffy_text::{self, Align, Settings};
use crate::style;

#[derive(Clone, Default)]
struct Dialog {
    settings: Settings,
    editing_run: Option<u32>,
    message: String,
}

fn id() -> egui::Id {
    egui::Id::new("puffy_text_parameters")
}

/// Open the preset (or edit the selected text group, including its child shapes).
pub fn open(app: &App, ctx: &egui::Context) {
    let (mut settings, editing_run) = match puffy_text::selected(&app.scene, app.sel) {
        Some(meta) => (meta.settings.clone(), Some(meta.run_id)),
        None => (Settings::default(), None),
    };
    if editing_run.is_none() {
        settings.anchor = [app.cam.target[0], app.cam.target[1]];
    }
    ctx.data_mut(|m| m.insert_temp(id(), Dialog { settings, editing_run, message: String::new() }));
    ctx.request_repaint();
}

/// In the selected group inspector, offer a direct route back to the text generator.
pub fn inspector_button(app: &mut App, ui: &mut egui::Ui, g: usize) {
    if app.scene.groups.get(g).and_then(|gr| gr.puffy_text.as_ref()).is_some() && ui.button("Puffy Text の文字・設定を再編集…").clicked()
    {
        open(app, ui.ctx());
    }
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(mut dlg) = ctx.data_mut(|m| m.remove_temp::<Dialog>(id())) else { return };
    let mut close = false;
    let mut apply = false;
    egui::Area::new(egui::Id::new("puffy_text_card")).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(385.0);
            ui.label(RichText::new("Puffy Text  ぷっくり文字").strong().size(16.0));
            ui.label(
                RichText::new(if dlg.editing_run.is_some() {
                    "文字列を再生成します。手動で移動・回転した各文字の位置はできるだけ保持します。"
                } else {
                    "フォントの輪郭から、ふくらんだ3D文字を作ります。日本語・英語に対応。"
                })
                .size(11.0)
                .color(style::DIM),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(540.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(RichText::new("文字（24文字まで）").size(12.0).color(style::DIM));
                ui.add(egui::TextEdit::singleline(&mut dlg.settings.text).desired_width(f32::INFINITY).hint_text("例：ぷっくり TEXT"));
                ui.add_space(3.0);
                ui.label(RichText::new("プレビュー：アプリUIのフォントです。3D形状は指定フォントで生成します。").size(10.0).color(style::DIM));
                let sample: String = dlg.settings.text.chars().take(24).collect();
                ui.label(RichText::new(if sample.is_empty() { "Aa あ".to_string() } else { sample }).size(26.0).color(style::ACCENT));

                ui.separator();
                ui.label(RichText::new("使うフォント（TTF・OTF・TTC）").size(12.0).color(style::DIM));
                ui.horizontal(|ui| {
                    if ui.button("フォントファイルを選択…").clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("フォント", &["ttf", "otf", "ttc"]).pick_file()
                    {
                        dlg.settings.font_path = path.to_string_lossy().into_owned();
                        dlg.settings.face_index = 0;
                    }
                    if ui.button("日本語フォントを探す").clicked() {
                        if let Some(path) = crate::fonts::find_cjk_font_path() {
                            dlg.settings.font_path = path.to_string_lossy().into_owned();
                            dlg.settings.face_index = 0;
                            dlg.message.clear();
                        } else {
                            dlg.message = "端末に対応フォントが見つかりません。TTF/OTF/TTCを指定してください".into();
                        }
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut dlg.settings.font_path).desired_width(f32::INFINITY).hint_text("フォントファイルの場所"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TTC内の書体番号").size(11.0).color(style::DIM));
                    ui.add(egui::DragValue::new(&mut dlg.settings.face_index).range(0..=32).speed(0.1));
                });
                ui.add_space(4.0);
                egui::Grid::new("puffy_text_settings_grid").num_columns(2).spacing([8.0, 7.0]).show(ui, |ui| {
                    ui.label("文字サイズ");
                    ui.add(egui::DragValue::new(&mut dlg.settings.size_mm).range(4.0..=100.0).speed(0.2).suffix(" mm").max_decimals(1))
                        .on_hover_text("フォントのemサイズ。全体の高さと幅に影響します");
                    ui.end_row();
                    ui.label("文字の厚さ");
                    ui.add(egui::DragValue::new(&mut dlg.settings.thickness_mm).range(0.8..=60.0).speed(0.2).suffix(" mm").max_decimals(1));
                    ui.end_row();
                    ui.label("ふくらみ");
                    ui.add(
                        egui::DragValue::new(&mut dlg.settings.puff_mm)
                            .range(0.0..=(dlg.settings.size_mm * 0.08).min(3.0))
                            .speed(0.05)
                            .suffix(" mm")
                            .max_decimals(2),
                    )
                    .on_hover_text("外へ丸くふくらませます。文字の穴がふさがる設定は拒否します");
                    ui.end_row();
                    ui.label("かどの丸み");
                    ui.add(egui::DragValue::new(&mut dlg.settings.corner_mm).range(0.0..=10.0).speed(0.05).suffix(" mm").max_decimals(2))
                        .on_hover_text("輪郭と表面の角を柔らかくします");
                    ui.end_row();
                    ui.label("文字間隔");
                    ui.add(egui::DragValue::new(&mut dlg.settings.tracking_mm).range(-3.0..=30.0).speed(0.2).suffix(" mm").max_decimals(1));
                    ui.end_row();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("位置合わせ");
                    for alignment in [Align::Left, Align::Center, Align::Right] {
                        if ui.selectable_label(dlg.settings.align == alignment, alignment.label()).clicked() {
                            dlg.settings.align = alignment;
                        }
                    }
                });
                ui.separator();
                ui.label(
                    RichText::new("各文字は別グループです。シーン一覧で文字のグループを選び、ギズモで移動・回転できます。")
                        .size(11.0)
                        .color(style::DIM),
                );
                ui.label(
                    RichText::new(
                        "フォントファイルは保存・配布しません。立体化した文字の商用利用・配布条件は各フォントのライセンスを確認してください。",
                    )
                    .size(11.0)
                    .color(style::DIM),
                );
                if !dlg.message.is_empty() {
                    ui.label(RichText::new(&dlg.message).color(style::CUT).size(12.0));
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let title = if dlg.editing_run.is_some() { "文字を更新" } else { "ぷっくり文字を作る" };
                let button = egui::Button::new(RichText::new(title).strong().color(Color32::WHITE))
                    .fill(style::ACCENT_SOFT)
                    .stroke(Stroke::new(1.0, style::ACCENT));
                if ui.add(button).clicked() {
                    apply = true;
                }
                if ui.button("キャンセル").clicked() {
                    close = true;
                }
            });
        });
    });
    if apply {
        match puffy_text::apply(&mut app.scene, &dlg.settings, dlg.editing_run) {
            Ok(pick) => {
                app.sel = Some(pick);
                app.frame_view(true);
                app.say(ctx, if dlg.editing_run.is_some() { "ぷっくり文字を更新しました" } else { "ぷっくり文字を作りました" });
                close = true;
            }
            Err(why) => {
                dlg.message = why;
            }
        }
    }
    if !close {
        ctx.data_mut(|m| m.insert_temp(id(), dlg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_can_open_without_a_font_and_cancel_does_not_change_the_scene() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let original = app.scene.clone();
        open(&app, &ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| update(&mut app, ui.ctx()));
        output.textures_delta.clear();
        assert!(ctx.data(|m| m.get_temp::<Dialog>(id())).is_some());
        assert_eq!(app.scene, original);
    }
}
