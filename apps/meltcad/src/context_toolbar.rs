//! Selection-aware quick actions shared by the floating toolbar and the viewport context menu.
//! Only tiny UI state is added: multi-selection is transient, never part of the saved project.
//! The original keyboard shortcuts, SDF rendering, and single-selection gizmos are unchanged.

use egui::{Color32, RichText, Ui, vec2};

use crate::app::{App, Tab};
use crate::gizmo::Tool;
use crate::model::{Kind, MAX_GROUPS, MAX_SHAPES, Pick};
use crate::sketch_ui::{Make, Stage};
use crate::style;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Move,
    Rotate,
    Scale,
    Duplicate,
    Delete,
    EditSketch,
    EndSketchEdit,
    Extrude,
    ExtrudeSettings,
    AlignX,
    AlignY,
    AlignZ,
    Group,
    Frame,
    Rename,
    Flip,
    Floor,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Move => crate::i18n::tr("context_toolbar.move"),
            Self::Rotate => crate::i18n::tr("context_toolbar.rotate"),
            Self::Scale => crate::i18n::tr("context_toolbar.scale"),
            Self::Duplicate => crate::i18n::tr("context_toolbar.duplicate"),
            Self::Delete => crate::i18n::tr("context_toolbar.delete"),
            Self::EditSketch => crate::i18n::tr("context_toolbar.edit_sketch"),
            Self::EndSketchEdit => crate::i18n::tr("context_toolbar.end_sketch_edit"),
            Self::Extrude => crate::i18n::tr("context_toolbar.extrude"),
            Self::ExtrudeSettings => crate::i18n::tr("context_toolbar.extrude_settings"),
            Self::AlignX => crate::i18n::tr("context_toolbar.align_x"),
            Self::AlignY => crate::i18n::tr("context_toolbar.align_y"),
            Self::AlignZ => crate::i18n::tr("context_toolbar.align_z"),
            Self::Group => crate::i18n::tr("context_toolbar.group"),
            Self::Frame => crate::i18n::tr("context_toolbar.frame"),
            Self::Rename => crate::i18n::tr("context_toolbar.rename"),
            Self::Flip => crate::i18n::tr("context_toolbar.flip"),
            Self::Floor => crate::i18n::tr("floor.button"),
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Move => crate::i18n::tr("context_toolbar.tip_move"),
            Self::Rotate => crate::i18n::tr("context_toolbar.tip_rotate"),
            Self::Scale => crate::i18n::tr("context_toolbar.tip_scale"),
            Self::Duplicate => crate::i18n::tr("context_toolbar.tip_duplicate"),
            Self::Delete => crate::i18n::tr("context_toolbar.tip_delete"),
            Self::EditSketch => crate::i18n::tr("context_toolbar.tip_edit_sketch"),
            Self::EndSketchEdit => crate::i18n::tr("context_toolbar.tip_end_sketch_edit"),
            Self::Extrude => crate::i18n::tr("context_toolbar.tip_extrude"),
            Self::ExtrudeSettings => crate::i18n::tr("context_toolbar.tip_extrude_settings"),
            Self::AlignX => crate::i18n::tr("context_toolbar.tip_align_x"),
            Self::AlignY => crate::i18n::tr("context_toolbar.tip_align_y"),
            Self::AlignZ => crate::i18n::tr("context_toolbar.tip_align_z"),
            Self::Group => crate::i18n::tr("context_toolbar.tip_group"),
            Self::Frame => crate::i18n::tr("context_toolbar.tip_frame"),
            Self::Rename => crate::i18n::tr("context_toolbar.tip_rename"),
            Self::Flip => crate::i18n::tr("context_toolbar.tip_flip"),
            Self::Floor => crate::i18n::tr("floor.tip"),
        }
    }
}

impl App {
    /// Non-mutating, bounded selection query: at most MAX_SHAPES items; no SDF rebuild.
    pub(crate) fn is_pick_selected(&self, pick: Pick) -> bool {
        self.sel == Some(pick) || (self.multi_for == self.sel && self.multi.contains(&pick))
    }

    /// Ordinary selection replaces the current selection; Ctrl-click toggles one extra item.
    /// The viewport keeps its Shift-click toggle and Ctrl/Cmd-click adds another pick.
    pub(crate) fn select_pick(&mut self, pick: Option<Pick>, additive: bool) {
        if !additive || pick.is_none() {
            self.sel = pick;
            self.multi.clear();
            self.multi_for = self.sel;
            return;
        }
        let Some(pick) = pick else { return };
        self.toggle_pick(pick);
    }

    /// Direct selection writes from older tools reset the extra picks automatically.
    pub(crate) fn sync_multi_selection(&mut self) {
        if self.sel != self.multi_for {
            self.multi.clear();
            self.multi_for = self.sel;
        }
        // a locked shape or group is never selected, whatever picked it
        if self.sel.is_some_and(|p| self.scene.is_locked(p)) {
            self.multi.retain(|p| !self.scene.is_locked(*p));
            self.sel = if self.multi.is_empty() { None } else { Some(self.multi.remove(0)) };
            self.multi_for = self.sel;
        }
        self.multi.retain(|p| {
            Some(*p) != self.sel
                && !self.scene.is_locked(*p)
                && match *p {
                    Pick::Shape(g, s) => self.scene.shape(g, s).is_some(),
                    Pick::Group(g) => g < self.scene.groups.len(),
                }
        });
    }

    pub(crate) fn clear_multi_selection(&mut self) {
        self.multi.clear();
        self.multi_for = self.sel;
    }

    fn selected_shape_keys(&self) -> Option<Vec<(usize, usize)>> {
        let Some(Pick::Shape(g, s)) = self.sel else { return None };
        self.scene.shape(g, s)?;
        let mut picks = vec![(g, s)];
        for p in &self.multi {
            let Pick::Shape(g, s) = *p else { return None };
            self.scene.shape(g, s)?;
            picks.push((g, s));
        }
        picks.sort_unstable();
        picks.dedup();
        Some(picks)
    }

    /// The UI asks this every frame. Check selection in O(k) without sorting or allocating.
    fn quick_group_check(&self) -> Result<(usize, usize), &'static str> {
        let Some(Pick::Shape(group, first)) = self.sel else {
            return Err(crate::i18n::tr("context_toolbar.err_select_multiple"));
        };
        if self.scene.shape(group, first).is_none() {
            return Err(crate::i18n::tr("context_toolbar.err_selection_missing"));
        }
        let mut count = 1;
        for &pick in &self.multi {
            let Pick::Shape(g, s) = pick else {
                return Err(crate::i18n::tr("context_toolbar.err_mixed_selection"));
            };
            if g != group {
                return Err(crate::i18n::tr("context_toolbar.err_different_groups"));
            }
            if self.scene.shape(g, s).is_none() {
                return Err(crate::i18n::tr("context_toolbar.err_selection_missing"));
            }
            count += 1;
        }
        if count < 2 {
            return Err(crate::i18n::tr("context_toolbar.err_select_two_or_more"));
        }
        Ok((group, count))
    }

    fn same_group_shapes(&self) -> Result<(usize, Vec<usize>), &'static str> {
        let keys = self.selected_shape_keys().ok_or(crate::i18n::tr("context_toolbar.err_select_multiple"))?;
        if keys.len() < 2 {
            return Err(crate::i18n::tr("context_toolbar.err_select_two_or_more"));
        }
        let group = keys[0].0;
        if keys.iter().any(|(g, _)| *g != group) {
            return Err(crate::i18n::tr("context_toolbar.err_different_groups"));
        }
        Ok((group, keys.into_iter().map(|(_, s)| s).collect()))
    }

    fn toolbar_actions(&self) -> Vec<Action> {
        if self.sk.on && self.sk.stage == Stage::Ready && !self.sk.profile.is_empty() {
            return vec![Action::Extrude, Action::EditSketch];
        }
        if !self.multi.is_empty() {
            return vec![Action::AlignX, Action::AlignY, Action::AlignZ, Action::Floor, Action::Group, Action::Duplicate, Action::Delete];
        }
        if self.sk.edit.is_some() {
            return vec![Action::EndSketchEdit, Action::Delete];
        }
        if let Some(Pick::Shape(g, s)) = self.sel
            && let Some(sh) = self.scene.shape(g, s)
            && crate::sketch::editable(sh)
        {
            return if sh.kind == Kind::Extrude {
                vec![
                    Action::EditSketch,
                    Action::ExtrudeSettings,
                    Action::Move,
                    Action::Rotate,
                    Action::Scale,
                    Action::Floor,
                    Action::Duplicate,
                    Action::Delete,
                ]
            } else {
                vec![Action::EditSketch, Action::Move, Action::Rotate, Action::Scale, Action::Floor, Action::Duplicate, Action::Delete]
            };
        }
        vec![Action::Move, Action::Rotate, Action::Scale, Action::Floor, Action::Duplicate, Action::Delete]
    }

    /// A reason makes the action disabled rather than silently ignoring it.
    fn disabled_reason(&self, action: Action) -> Option<&'static str> {
        if self.sel.is_none() && !(action == Action::Extrude && self.sk.on) {
            return Some(crate::i18n::tr("context_toolbar.err_select_shape"));
        }
        if self.drag.is_some() && !matches!(action, Action::Frame) {
            return Some(crate::i18n::tr("context_toolbar.err_finish_drag"));
        }
        if self.scene_structure_locked() && !matches!(action, Action::Frame | Action::Rename | Action::EndSketchEdit) {
            return Some(crate::i18n::tr("context_toolbar.err_finish_active_sketch_or_solid"));
        }
        match action {
            Action::Move | Action::Rotate | Action::Scale => {
                if self.sketch_active() {
                    Some(crate::i18n::tr("context_toolbar.err_gizmo_in_sketch"))
                } else {
                    None
                }
            }
            Action::Duplicate => {
                if !self.multi.is_empty() {
                    let Ok((_, count)) = self.quick_group_check() else {
                        return Some(crate::i18n::tr("context_toolbar.err_duplicate_same_group"));
                    };
                    if self.scene.shape_count().saturating_add(count) > MAX_SHAPES {
                        return Some(crate::i18n::tr("context_toolbar.err_shape_limit_would_reach"));
                    }
                } else {
                    match self.sel {
                        Some(Pick::Shape(..)) if self.scene.shape_count() >= MAX_SHAPES => {
                            return Some(crate::i18n::tr("context_toolbar.err_shape_limit_reached"));
                        }
                        Some(Pick::Group(g)) => {
                            if self.scene.groups.len() >= MAX_GROUPS {
                                return Some(crate::i18n::tr("context_toolbar.err_group_limit_reached"));
                            }
                            let count = self.scene.groups.get(g).map_or(0, |gr| gr.shapes.len());
                            if self.scene.shape_count().saturating_add(count) > MAX_SHAPES {
                                return Some(crate::i18n::tr("context_toolbar.err_shape_limit_would_reach"));
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            Action::Delete => {
                if self.sk.edit.is_some() {
                    return Some(crate::i18n::tr("context_toolbar.err_outline_editing"));
                }
                if self.multi.is_empty() || (matches!(self.sel, Some(Pick::Shape(..))) && self.multi.iter().all(|p| matches!(p, Pick::Shape(..)))) {
                    None
                } else {
                    Some(crate::i18n::tr("context_toolbar.err_delete_mixed"))
                }
            }
            Action::EditSketch => match self.sel {
                Some(Pick::Shape(g, s)) if self.scene.shape(g, s).is_some_and(crate::sketch::editable) => None,
                _ => Some(crate::i18n::tr("context_toolbar.err_select_sketch_shape")),
            },
            Action::EndSketchEdit => self.sk.edit.is_none().then_some(crate::i18n::tr("context_toolbar.err_not_editing_outline")),
            Action::Extrude => {
                if self.sk.on && self.sk.keep_sketch {
                    Some(crate::i18n::tr("sketch.finish_first"))
                } else if !(self.sk.on && self.sk.stage == Stage::Ready && !self.sk.profile.is_empty()) {
                    Some(crate::i18n::tr("context_toolbar.err_complete_2d_outline"))
                } else if self.scene.shape_count().saturating_add(1 + self.sk.holes.len()) > MAX_SHAPES {
                    Some(crate::i18n::tr("context_toolbar.err_shape_limit_reached"))
                } else if !self.sk.holes.is_empty() && self.scene.groups.len() >= MAX_GROUPS {
                    Some(crate::i18n::tr("context_toolbar.err_group_needed_for_hole_sketch"))
                } else {
                    None
                }
            }
            Action::ExtrudeSettings => match self.sel {
                Some(Pick::Shape(g, s)) if self.scene.shape(g, s).is_some_and(|s| s.kind == Kind::Extrude) => None,
                _ => Some(crate::i18n::tr("context_toolbar.err_select_extruded_shape")),
            },
            Action::AlignX | Action::AlignY | Action::AlignZ => self.quick_group_check().err(),
            Action::Group => {
                let Ok((g, _)) = self.quick_group_check() else {
                    return Some(crate::i18n::tr("context_toolbar.err_select_two_same_group"));
                };
                if self.scene.groups.len() >= MAX_GROUPS {
                    return Some(crate::i18n::tr("context_toolbar.err_group_limit_reached"));
                }
                let src = &self.scene.groups[g];
                if src.pattern.is_some() || src.instance_of.is_some() || src.sketch_holes {
                    return Some(crate::i18n::tr("context_toolbar.err_cannot_split_pattern"));
                }
                if src.uid != 0 && self.scene.groups.iter().any(|gr| gr.instance_of == Some(src.uid)) {
                    return Some(crate::i18n::tr("context_toolbar.err_cannot_split_link_source"));
                }
                None
            }
            Action::Floor => None,
            Action::Frame | Action::Rename | Action::Flip => {
                if !self.multi.is_empty() && matches!(action, Action::Flip | Action::Frame) {
                    Some(crate::i18n::tr("context_toolbar.err_select_one_multi"))
                } else {
                    None
                }
            }
        }
    }

    fn run_toolbar_action(&mut self, action: Action, ctx: &egui::Context) {
        if let Some(why) = self.disabled_reason(action) {
            self.say(ctx, why);
            return;
        }
        match action {
            Action::Move => self.tool = Tool::Move,
            Action::Rotate => self.tool = Tool::Rotate,
            Action::Scale => self.tool = Tool::Scale,
            Action::Duplicate => {
                self.duplicate();
            }
            Action::Delete => self.delete_selected(),
            Action::EditSketch => {
                if let Some(Pick::Shape(g, s)) = self.sel {
                    self.sketch_edit_begin(g, s);
                }
            }
            Action::EndSketchEdit => self.sk.edit = None,
            Action::Extrude => {
                self.sk.make = Make::Extrude;
                self.sketch_make(ctx);
            }
            Action::ExtrudeSettings => self.tab = Tab::Shape,
            Action::AlignX => self.align_multi(0),
            Action::AlignY => self.align_multi(1),
            Action::AlignZ => self.align_multi(2),
            Action::Group => self.group_multi(ctx),
            Action::Frame => self.frame_view(true),
            Action::Rename => self.renaming = self.sel,
            Action::Flip => self.flip_op(),
            Action::Floor => self.place_on_floor(ctx),
        }
    }

    /// Put the selection on the floor: its lowest point comes to height 0.
    pub(crate) fn place_on_floor(&mut self, ctx: &egui::Context) {
        let targets = crate::floor::targets(&self.scene, &self.picks());
        match crate::floor::drop(&mut self.scene, &targets) {
            Ok(d) if d.abs() < 0.005 => self.say(ctx, crate::i18n::tr("floor.already")),
            Ok(_) => self.say(ctx, crate::i18n::tr("floor.done")),
            Err(why) => self.say(ctx, why),
        }
    }

    fn align_multi(&mut self, axis: usize) {
        let Some(Pick::Shape(g, s)) = self.sel else { return };
        let Some(value) = self.scene.shape(g, s).map(|shape| shape.pos[axis]) else { return };
        let Ok((group, keys)) = self.same_group_shapes() else { return };
        for si in keys {
            if let Some(shape) = self.scene.shape_mut(group, si) {
                shape.pos[axis] = value;
            }
        }
    }

    pub(crate) fn duplicate_multi(&mut self) {
        if !self.allow_scene_structure_change() {
            return;
        }
        let Ok((g, keys)) = self.same_group_shapes() else { return };
        if self.scene.shape_count().saturating_add(keys.len()) > MAX_SHAPES {
            self.notice = Some(crate::i18n::tr("context_toolbar.err_shape_limit_would_reach").into());
            return;
        }
        let copies: Vec<_> = keys.iter().filter_map(|s| self.scene.shape(g, *s).cloned()).collect();
        if let Err(why) = crate::mesh::can_add(&self.scene, copies.iter()) {
            self.notice = Some(why.into());
            return;
        }
        let mut copies = copies;
        for (index, mut copy) in keys.iter().copied().zip(copies.drain(..)).rev() {
            // A duplicate is a new animation/pattern identity, not another reference to its source.
            copy.uid = 0;
            copy.pos[0] += 8.0;
            copy.pos[1] += 8.0;
            copy.name.push_str(crate::i18n::tr("context_toolbar.copy_suffix"));
            self.scene.groups[g].shapes.insert(index + 1, copy);
        }
        self.sel = Some(Pick::Shape(g, keys[0] + 1));
        self.clear_multi_selection();
    }

    fn group_multi(&mut self, ctx: &egui::Context) {
        if let Some(why) = self.disabled_reason(Action::Group) {
            self.say(ctx, why);
            return;
        }
        self.group_selected();
        self.say(ctx, crate::i18n::tr("context_toolbar.grouped_message"));
    }

    fn button_action(&mut self, ui: &mut Ui, action: Action, menu: bool) {
        let why = self.disabled_reason(action);
        let label = if menu {
            match action {
                Action::Move => crate::i18n::tr("context_toolbar.label_move"),
                Action::Rotate => crate::i18n::tr("context_toolbar.label_rotate"),
                Action::Scale => crate::i18n::tr("context_toolbar.label_scale"),
                Action::Duplicate => crate::i18n::tr("context_toolbar.label_duplicate"),
                Action::Delete => crate::i18n::tr("context_toolbar.label_delete"),
                _ => action.label(),
            }
        } else {
            action.label()
        };
        let button = egui::Button::new(RichText::new(label).size(if menu { 13.0 } else { 12.5 }).color(if action == Action::Delete {
            style::CUT
        } else {
            style::TEXT
        }));
        let response = ui.add_enabled(why.is_none(), button);
        // Disabled egui widgets use a separate hover path; plain on_hover_text
        // does not reliably display the reason when the button is inactive.
        let response = match why {
            Some(reason) => response.on_disabled_hover_text(reason),
            None => response.on_hover_text(action.hint()),
        };
        if response.clicked() {
            let ctx = ui.ctx().clone();
            self.run_toolbar_action(action, &ctx);
            if menu {
                ui.close();
            }
        }
    }

    /// One dispatcher provides identical actions in the floating toolbar and right click.
    pub(crate) fn context_actions(&mut self, ui: &mut Ui) {
        if self.sel.is_none() && !(self.sk.on && self.sk.stage == Stage::Ready) {
            ui.label(RichText::new(crate::i18n::tr("context_toolbar.hint_select_to_edit")).color(style::DIM));
            return;
        }
        // a solid made in SolidCraft: open its design there again
        if let Some(Pick::Shape(g, s)) = self.sel
            && self.scene.shape(g, s).is_some_and(|x| x.solid.is_some())
        {
            let r = ui
                .horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(vec2(16.0, 18.0), egui::Sense::hover());
                    style::solidcraft_logo(ui.painter(), logo);
                    ui.button(RichText::new(crate::i18n::tr("menu.edit")).color(style::ACCENT))
                })
                .inner;
            if r.on_hover_text(crate::i18n::tr("solid.open")).clicked() {
                let ctx = ui.ctx().clone();
                if let Err(e) = self.edit_in_solidcraft(&ctx, g, s) {
                    self.say(&ctx, e);
                }
                ui.close();
                return;
            }
            ui.separator();
        }
        for action in self.toolbar_actions() {
            self.button_action(ui, action, true);
        }
        ui.separator();
        for action in [Action::Frame, Action::Rename, Action::Flip] {
            self.button_action(ui, action, true);
        }
    }

    /// Display a low-cost marker for each additional selected shape (GPU picking still uses primary).
    pub(crate) fn paint_multi_selection(&self, painter: &egui::Painter, rect: egui::Rect) {
        for pick in &self.multi {
            let Pick::Shape(g, s) = *pick else { continue };
            let Some(shape) = self.scene.shape(g, s) else { continue };
            let Some(pos) = self.cam.project(rect, shape.pos) else { continue };
            if !rect.contains(pos) {
                continue;
            }
            painter.circle_stroke(pos, 9.0, egui::Stroke::new(2.0, style::ACCENT));
            painter.circle_filled(pos, 2.0, Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Material, Shape};
    use egui::pos2;
    use std::time::Instant;

    #[test]
    fn actions_follow_kind_and_preserve_original_shortcut_tools() {
        let mut app = App::new(true, None);
        assert_eq!(app.toolbar_actions(), vec![Action::Move, Action::Rotate, Action::Scale, Action::Floor, Action::Duplicate, Action::Delete]);
        // the floor button puts the starting sphere on the floor
        let ctx = egui::Context::default();
        app.scene.groups[0].shapes[0].pos[2] += 40.0;
        app.run_toolbar_action(Action::Floor, &ctx);
        let bottom = crate::floor::lowest(&app.scene, &[(0, 0)]).expect("a bottom");
        assert!(bottom.abs() < 0.1, "bottom at {bottom}");
        let ctx = egui::Context::default();
        app.run_toolbar_action(Action::Rotate, &ctx);
        assert_eq!(app.tool, Tool::Rotate);
        app.run_toolbar_action(Action::Move, &ctx);
        assert_eq!(app.tool, Tool::Move);
        app.scene.groups[0].shapes.push(Shape::new(Kind::Extrude, [0.0; 3], Material::default()));
        app.select_pick(Some(Pick::Shape(0, 1)), false);
        assert!(app.toolbar_actions().contains(&Action::EditSketch));
        assert!(app.toolbar_actions().contains(&Action::ExtrudeSettings));
        app.select_pick(None, false);
        assert!(app.toolbar_actions().contains(&Action::Move));
    }

    #[test]
    fn is_pick_selected_answers_exactly_what_the_pick_list_contains() {
        // The scene list asks this once per row and frame (no allocation); it must agree with
        // `picks().contains()` in every state of the selection.
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [20.0, 7.0, 9.0], Material::default()));
        app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [-14.0, 11.0, 12.0], Material::default()));
        app.settle(&ctx);
        let all = [Pick::Shape(0, 0), Pick::Shape(0, 1), Pick::Shape(0, 2), Pick::Group(0), Pick::Group(7)];
        let check = |app: &App, what: &str| {
            for p in all {
                assert_eq!(app.is_pick_selected(p), app.picks().contains(&p), "{what}: {p:?}");
            }
        };
        check(&app, "nothing picked");
        app.select_pick(Some(Pick::Shape(0, 1)), false);
        check(&app, "one picked");
        app.select_pick(Some(Pick::Shape(0, 2)), true);
        check(&app, "two picked");
        app.toggle_pick(Pick::Shape(0, 0));
        check(&app, "three picked");
        app.toggle_pick(Pick::Shape(0, 2));
        check(&app, "two picked again");
        app.select_pick(Some(Pick::Shape(0, 1)), false);
        check(&app, "back to one");
        app.select_pick(None, false);
        check(&app, "cleared");
    }

    #[test]
    fn multiselect_toggle_align_duplicate_delete_group_and_undo() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [20.0, 7.0, 9.0], Material::default()));
        app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [-14.0, 11.0, 12.0], Material::default()));
        for si in 0..3 {
            app.scene.ensure_shape_uid(0, si).expect("original shape gets a stable identity");
        }
        let original_uids: Vec<_> = (0..3).map(|si| app.scene.shape(0, si).unwrap().uid).collect();
        app.settle(&ctx); // Establish the scene before testing a single undo transaction.
        app.select_pick(Some(Pick::Shape(0, 0)), false);
        app.select_pick(Some(Pick::Shape(0, 1)), true);
        app.select_pick(Some(Pick::Shape(0, 2)), true);
        assert_eq!(app.multi.len(), 2);
        let anchor = app.scene.shape(0, 0).unwrap().pos[0];
        app.run_toolbar_action(Action::AlignX, &ctx);
        assert_eq!(app.scene.shape(0, 1).unwrap().pos[0], anchor);
        assert_eq!(app.scene.shape(0, 2).unwrap().pos[0], anchor);
        app.run_toolbar_action(Action::Duplicate, &ctx);
        assert_eq!(app.scene.shape_count(), 6);
        assert_eq!(
            [app.scene.shape(0, 1).unwrap().uid, app.scene.shape(0, 3).unwrap().uid, app.scene.shape(0, 5).unwrap().uid],
            [0; 3],
            "duplicates must not inherit animation or pattern identity"
        );
        assert_eq!(
            vec![app.scene.shape(0, 0).unwrap().uid, app.scene.shape(0, 2).unwrap().uid, app.scene.shape(0, 4).unwrap().uid],
            original_uids,
            "the originals keep their identities"
        );
        assert!(app.multi.is_empty());
        app.settle(&ctx);
        app.undo();
        assert_eq!(app.scene.shape_count(), 3);
        app.select_pick(Some(Pick::Shape(0, 0)), false);
        app.select_pick(Some(Pick::Shape(0, 2)), true);
        app.run_toolbar_action(Action::Group, &ctx);
        assert_eq!(app.scene.groups.len(), 2);
        assert_eq!(app.scene.groups[1].shapes.len(), 2);
        assert_eq!(app.scene.shape_count(), 3);
        app.select_pick(Some(Pick::Shape(1, 0)), false);
        app.select_pick(Some(Pick::Shape(1, 1)), true);
        app.run_toolbar_action(Action::Delete, &ctx);
        assert_eq!(app.scene.groups[1].shapes.len(), 0);
        assert!(app.multi.is_empty());
    }

    #[test]
    fn plain_sketch_must_be_saved_before_extrusion_and_invalid_dimensions_do_not_commit() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.set_sketch_tool(true);
        app.sk.stage = Stage::Ready;
        app.sk.profile = crate::sketch::rect([0.0, 0.0], [20.0, 20.0]);
        let before = app.scene.clone();

        assert!(app.disabled_reason(Action::Extrude).is_some());
        app.run_toolbar_action(Action::Extrude, &ctx);
        assert_eq!(app.scene, before, "the toolbar cannot bypass Finish Sketch");
        assert_eq!(app.sk.make, Make::Extrude);

        app.sk.dimension =
            crate::sketch_dimensions::Dimensions::for_shape(crate::sketch_ui::Pen::Rect, [0.0, 0.0], [20.0, 20.0], &app.sk.profile, false);
        app.sk.dimension.as_mut().unwrap().fields[0] = "not a number".into();
        assert!(!app.sketch_confirm(&ctx));
        assert_eq!(app.scene, before, "invalid dimensions cannot create stale guide geometry");
        assert!(app.sk.keep_sketch);

        app.sk.dimension = None;
        assert!(app.sketch_confirm(&ctx));
        assert!(!app.sk.on);
        assert_eq!(app.scene.shape_count(), before.shape_count() + 1);
        assert!(app.scene.groups.iter().flat_map(|g| g.shapes.iter()).any(|shape| shape.guide));
    }

    #[test]
    fn disabled_actions_explain_why() {
        let mut app = App::new(true, None);
        assert!(app.disabled_reason(Action::AlignX).is_some());
        app.select_pick(Some(Pick::Shape(0, 0)), false);
        app.select_pick(Some(Pick::Group(0)), true);
        assert!(app.disabled_reason(Action::Group).is_some());
        assert!(app.disabled_reason(Action::Delete).is_some());
    }

    /// EgUI tree rows plus toolbar, not just the constant-time action selector.
    /// The numbers are logged on the runner; no hardware-dependent pass threshold.
    #[test]
    fn toolbar_and_scene_list_160_objects_ui_p95() {
        fn sample(count: usize) -> (f64, f64) {
            let ctx = egui::Context::default();
            crate::style::apply(&ctx);
            let mut app = App::new(true, None);
            for i in 1..count {
                app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [i as f32, 0.0, 10.0], Material::default()));
            }
            app.view_rect = egui::Rect::from_min_size(pos2(280.0, 45.0), vec2(550.0, 640.0));
            let mut times = Vec::new();
            for i in 0..24 {
                app.select_pick(Some(Pick::Shape(0, i % count)), false);
                let started = Instant::now();
                let mut output = ctx.run_ui(
                    egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1100.0, 760.0))), ..Default::default() },
                    |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(272.0);
                            app.layers(ui);
                        });
                    },
                );
                output.textures_delta.clear();
                if i > 3 {
                    times.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            let mid = times[times.len() / 2];
            let p95 = times[((times.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
            (mid, p95)
        }
        let one = sample(1);
        let many = sample(160);
        eprintln!(
            "YuraCAD context UI: 1 shape p50={:.3}ms p95={:.3}ms; 160 shapes p50={:.3}ms p95={:.3}ms (debug runner, not GPU latency)",
            one.0, one.1, many.0, many.1
        );
        assert!(many.0.is_finite() && many.1.is_finite());
    }

    /// Test selection changes, action filtering and drawing controls without timing an SDF bake.
    /// Report elapsed time rather than using a brittle millisecond pass/fail threshold.
    #[test]
    fn toolbar_selection_latency_with_160_shapes() {
        let mut app = App::new(true, None);
        for i in 1..160 {
            app.scene.groups[0].shapes.push(Shape::new(Kind::Box, [i as f32, 0.0, 0.0], Material::default()));
        }
        let started = Instant::now();
        for i in 0..500 {
            app.select_pick(Some(Pick::Shape(0, i % 160)), false);
            app.sync_multi_selection();
            let actions = app.toolbar_actions();
            assert_eq!(actions.len(), 6);
            for action in actions {
                let _ = app.disabled_reason(action);
            }
        }
        let elapsed = started.elapsed();
        eprintln!(
            "YuraCAD toolbar 160 shapes: 500 selection updates in {:.3} ms ({:.3} ms/update)",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_secs_f64() * 2.0
        );
        // Not a performance assertion: CI hardware / debug builds vary widely.
        assert_eq!(app.scene.shape_count(), 160);
    }
}
