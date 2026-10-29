//! The Preferences card: language first, then units and display, snapping, navigation, saving and
//! export defaults. Every control is a visible pointer target (no shortcut is required), changes
//! apply immediately, Cancel restores the opening snapshot and Reset returns every value to its
//! default. The card uses the same flat, raised card and pill controls as the other Melt cards.

use std::collections::BTreeMap;

use egui::{Align2, Key, Rect, RichText, Sense, Ui, vec2};

use crate::app::App;
use crate::i18n::tr;
use crate::prefs::{
    DisplayUnits, LANGS, MAX_AUTOSAVE_MINUTES, MAX_DECIMALS, MAX_GRID_MM, MAX_ORBIT, MIN_AUTOSAVE_MINUTES, MIN_GRID_MM, MIN_ORBIT, Navigation,
    Preferences,
};
use crate::style;

/// The opening snapshot. The live preferences stay in `App`; this only feeds Cancel.
#[derive(Clone)]
struct Dlg {
    original: Preferences,
}

fn id() -> egui::Id {
    egui::Id::new("meltcad_preferences_card")
}

fn area_id() -> egui::Id {
    egui::Id::new("meltcad_preferences_area")
}

fn hits_id() -> egui::Id {
    egui::Id::new("meltcad_preferences_controls")
}

#[derive(Clone, Default)]
struct Hits(BTreeMap<String, Rect>);

fn begin_frame(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(hits_id(), Hits::default()));
}

fn record(ctx: &egui::Context, id: &str, rect: Rect) {
    ctx.data_mut(|d| {
        let mut hits = d.get_temp::<Hits>(hits_id()).unwrap_or_default();
        hits.0.insert(id.to_owned(), rect);
        d.insert_temp(hits_id(), hits);
    });
}

/// Every pointer target the card must show. Tests assert each one was laid out.
#[cfg(test)]
pub const CONTROL_IDS: &[&str] = &[
    "settings.lang.ja",
    "settings.lang.en",
    "settings.lang.zh-Hans",
    "settings.lang.ko",
    "settings.lang.de",
    "settings.lang.fr",
    "settings.lang.es",
    "settings.units.mm",
    "settings.units.cm",
    "settings.units.m",
    "settings.units.in",
    "settings.decimals",
    "settings.grid.on",
    "settings.grid.off",
    "settings.grid.size",
    "settings.snap.grid.on",
    "settings.snap.grid.off",
    "settings.snap.vertex.on",
    "settings.snap.vertex.off",
    "settings.nav.melt",
    "settings.nav.cad",
    "settings.orbit",
    "settings.autosave",
    "settings.export.format.stl",
    "settings.export.format.3mf",
    "settings.export.format.obj",
    "settings.export.format.ply",
    "settings.export.format.glb",
    "settings.export.format.stl-ascii",
    "settings.export.units.mm",
    "settings.export.units.cm",
    "settings.export.units.m",
    "settings.export.units.in",
    "settings.export.up.z",
    "settings.export.up.y",
    "settings.reset",
    "settings.cancel",
    "settings.close",
];

pub fn open(ctx: &egui::Context, original: &Preferences) {
    let d = Dlg { original: original.clone() };
    ctx.data_mut(|m| m.insert_temp(id(), d));
    // The card is auto-sized; draw the settling frame now rather than waiting for the next click.
    ctx.request_repaint();
}

pub fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|m| m.get_temp::<Dlg>(id())).is_some()
}

/// The rect a control occupied in the last frame (tests use real pointer presses).
#[cfg(test)]
pub fn hit(ctx: &egui::Context, id: &str) -> Option<Rect> {
    ctx.data(|m| m.get_temp::<Hits>(hits_id())).and_then(|h| h.0.get(id).copied())
}

fn section(ui: &mut Ui, title: &str, hint: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let (bar, _) = ui.allocate_exact_size(vec2(3.0, 15.0), Sense::hover());
        ui.painter().rect_filled(bar, 1.5, style::ACCENT);
        ui.label(RichText::new(tr(title)).strong().size(14.0).color(style::TEXT));
    });
    ui.label(RichText::new(tr(hint)).size(11.0).color(style::DIM));
    ui.add_space(3.0);
}

fn bool_pair(ui: &mut Ui, prefix: &str, value: bool) -> bool {
    let on = style::pill(ui, tr("settings.on"), value);
    record(ui.ctx(), &format!("{prefix}.on"), on.rect);
    let off = style::pill(ui, tr("settings.off"), !value);
    record(ui.ctx(), &format!("{prefix}.off"), off.rect);
    if on.clicked() {
        true
    } else if off.clicked() {
        false
    } else {
        value
    }
}

pub fn update(app: &mut App, ctx: &egui::Context) {
    let Some(d) = ctx.data(|m| m.get_temp::<Dlg>(id())) else {
        begin_frame(ctx);
        return;
    };
    begin_frame(ctx);
    let mut next = app.preferences.clone();
    let (mut close, mut cancel, mut reset, mut reset_done) = (false, false, false, false);
    let mut open = true;
    egui::Area::new(area_id()).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        style::card().show(ui, |ui| {
            ui.set_width(548.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("settings.title")).strong().size(19.0).color(style::TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(tr("settings.live_note")).size(11.0).color(style::ACCENT));
                });
            });
            ui.label(RichText::new(tr("settings.subtitle")).size(11.5).color(style::DIM));
            ui.add_space(4.0);
            style::hsep_w(ui, 524.0);
            let max_h = (ctx.content_rect().height() - 145.0).clamp(240.0, 1400.0);
            // A fixed-size host stops the auto-sized Area and the scroll area from feeding each
            // other's height back every frame (which used to make the card creep upward).
            ui.allocate_ui_with_layout(vec2(524.0, max_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_size(vec2(524.0, max_h));
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, false]).show(ui, |ui| {
                    // Language is intentionally first: it changes the rest of the card next frame.
                    section(ui, "settings.language", "settings.language_hint");
                    let mut lang_changed = false;
                    ui.horizontal_wrapped(|ui| {
                        for l in LANGS {
                            let native = crate::i18n::language_name(l.id);
                            let shown = if native.is_empty() { l.native } else { native };
                            let resp = style::pill(ui, shown, next.lang == l.id);
                            record(ui.ctx(), &format!("settings.lang.{}", l.id), resp.rect);
                            if resp.clicked() && next.set_language(l.id) {
                                lang_changed = true;
                            }
                        }
                    });
                    if lang_changed {
                        ctx.request_repaint();
                    }

                    section(ui, "settings.units", "settings.units_hint");
                    let units = [(DisplayUnits::Mm, "mm"), (DisplayUnits::Cm, "cm"), (DisplayUnits::M, "m"), (DisplayUnits::Inch, "inch")];
                    ui.horizontal_wrapped(|ui| {
                        for (unit, label) in units {
                            let resp = style::pill(ui, label, next.units() == unit);
                            record(ui.ctx(), &format!("settings.units.{}", unit.id()), resp.rect);
                            if resp.clicked() {
                                next.units = unit.id().to_owned();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.decimals")).size(12.5).color(style::TEXT));
                        let resp = ui.add(egui::Slider::new(&mut next.decimals, 0..=MAX_DECIMALS).text(tr("settings.decimals_unit")));
                        record(ui.ctx(), "settings.decimals", resp.rect);
                    });
                    ui.label(
                        RichText::new(format!("{}: {}", tr("settings.units_example"), next.units().length(30.0, next.decimals())))
                            .size(11.0)
                            .color(style::DIM),
                    );

                    section(ui, "settings.theme", "settings.theme_hint");
                    ui.label(RichText::new(tr("settings.theme_dark")).size(12.5).color(style::DIM));

                    section(ui, "settings.grid", "settings.grid_hint");
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.grid_show")).size(12.5).color(style::TEXT));
                        next.grid = bool_pair(ui, "settings.grid", next.grid);
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.grid_size")).size(12.5).color(style::TEXT));
                        let resp =
                            ui.add(egui::Slider::new(&mut next.grid_size_mm, MIN_GRID_MM..=MAX_GRID_MM).step_by(1.0).suffix(" mm").max_decimals(0));
                        record(ui.ctx(), "settings.grid.size", resp.rect);
                    });

                    section(ui, "settings.snap", "settings.snap_hint");
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.snap_grid")).size(12.5).color(style::TEXT));
                        next.snap_grid = bool_pair(ui, "settings.snap.grid", next.snap_grid);
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.snap_vertex")).size(12.5).color(style::TEXT));
                        next.snap_vertex = bool_pair(ui, "settings.snap.vertex", next.snap_vertex);
                    });

                    section(ui, "settings.nav", "settings.nav_hint");
                    ui.horizontal_wrapped(|ui| {
                        for (nav, label) in [(Navigation::Melt, tr("settings.nav_melt")), (Navigation::Cad, tr("settings.nav_cad"))] {
                            let resp = style::pill(ui, label, next.navigation() == nav);
                            record(ui.ctx(), &format!("settings.nav.{}", nav.id()), resp.rect);
                            if resp.clicked() {
                                next.nav = nav.id().to_owned();
                            }
                        }
                    });
                    let nav_help = if next.navigation() == Navigation::Melt { "settings.nav_melt_help" } else { "settings.nav_cad_help" };
                    ui.label(RichText::new(tr(nav_help)).size(11.0).color(style::DIM));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.orbit")).size(12.5).color(style::TEXT));
                        let resp =
                            ui.add(egui::Slider::new(&mut next.orbit_sensitivity, MIN_ORBIT..=MAX_ORBIT).step_by(0.05).suffix("×").max_decimals(2));
                        record(ui.ctx(), "settings.orbit", resp.rect);
                    });

                    section(ui, "settings.saving", "settings.saving_hint");
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("settings.autosave")).size(12.5).color(style::TEXT));
                        let unit = format!(" {}", tr("settings.autosave_unit"));
                        let resp = ui.add(
                            egui::Slider::new(&mut next.autosave_minutes, MIN_AUTOSAVE_MINUTES..=MAX_AUTOSAVE_MINUTES).step_by(0.5).suffix(unit),
                        );
                        record(ui.ctx(), "settings.autosave", resp.rect);
                    });
                    ui.label(RichText::new(tr("settings.autosave_help")).size(11.0).color(style::DIM));

                    section(ui, "settings.export", "settings.export_hint");
                    ui.label(RichText::new(tr("settings.export_format")).size(12.0).color(style::DIM));
                    ui.horizontal_wrapped(|ui| {
                        for f in crate::export_formats::Format::ALL {
                            let resp = style::pill(ui, f.label(), next.export_format() == f);
                            record(ui.ctx(), &format!("settings.export.format.{}", f.id()), resp.rect);
                            if resp.clicked() {
                                next.export_format = f.id().to_owned();
                            }
                        }
                    });
                    ui.add_space(3.0);
                    ui.label(RichText::new(tr("settings.export_units")).size(12.0).color(style::DIM));
                    ui.horizontal_wrapped(|ui| {
                        for u in crate::export_formats::Units::ALL {
                            let resp = style::pill(ui, u.label(), next.export_units() == u);
                            record(ui.ctx(), &format!("settings.export.units.{}", u.id()), resp.rect);
                            if resp.clicked() {
                                next.export_units = u.id().to_owned();
                            }
                        }
                    });
                    ui.add_space(3.0);
                    ui.label(RichText::new(tr("settings.export_up")).size(12.0).color(style::DIM));
                    ui.horizontal_wrapped(|ui| {
                        for (up, label) in
                            [(crate::export_job::Up::Z, tr("settings.export_up_z")), (crate::export_job::Up::Y, tr("settings.export_up_y"))]
                        {
                            let resp = style::pill(ui, label, next.export_up() == up);
                            record(ui.ctx(), &format!("settings.export.up.{}", up.id()), resp.rect);
                            if resp.clicked() {
                                next.export_up = up.id().to_owned();
                            }
                        }
                    });
                });
            });

            style::hsep_w(ui, 524.0);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let resp = style::pill(ui, tr("settings.reset"), false);
                record(ctx, "settings.reset", resp.rect);
                if resp.on_hover_text(tr("settings.reset_tip")).clicked() {
                    reset = true;
                    reset_done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = style::pill(ui, tr("settings.close"), false);
                    record(ctx, "settings.close", resp.rect);
                    if resp.on_hover_text(tr("settings.close_tip")).clicked() {
                        close = true;
                    }
                    let resp = style::pill(ui, tr("settings.cancel"), false);
                    record(ctx, "settings.cancel", resp.rect);
                    if resp.on_hover_text(tr("settings.cancel_tip")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    });

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        cancel = true;
    }
    if reset {
        next = Preferences::default();
    }
    if cancel {
        next = d.original.clone();
        close = true;
    }
    if next != app.preferences {
        app.preferences = next;
        app.apply_preferences();
        ctx.request_repaint();
    }
    if reset_done {
        app.say(ctx, tr("settings.reset_done"));
    }
    if close {
        open = false;
    }
    if open {
        ctx.data_mut(|m| m.insert_temp(id(), Dlg { original: d.original }));
    } else {
        ctx.data_mut(|m| m.remove::<Dlg>(id()));
    }
}

// Menu recording is used only by the UI test that checks Settings sits below About.
#[cfg(test)]
#[derive(Clone, Default)]
struct MenuHits(BTreeMap<String, Rect>);

#[cfg(test)]
fn menu_id() -> egui::Id {
    egui::Id::new("meltcad_menu_hits")
}

#[cfg(test)]
fn anchor_id() -> egui::Id {
    egui::Id::new("meltcad_menu_anchor_id")
}

#[cfg(test)]
pub(crate) fn clear_menu(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(menu_id(), MenuHits::default()));
}

#[cfg(test)]
pub(crate) fn record_menu(ctx: &egui::Context, id: &str, rect: Rect) {
    ctx.data_mut(|d| {
        let mut hits = d.get_temp::<MenuHits>(menu_id()).unwrap_or_default();
        hits.0.insert(id.to_owned(), rect);
        d.insert_temp(menu_id(), hits);
    });
}

#[cfg(test)]
pub(crate) fn menu_hit(ctx: &egui::Context, id: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<MenuHits>(menu_id())).and_then(|h| h.0.get(id).copied())
}

#[cfg(test)]
pub(crate) fn record_menu_anchor(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|d| d.insert_temp(anchor_id(), id));
}

#[cfg(test)]
pub(crate) fn menu_anchor(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|d| d.get_temp::<egui::Id>(anchor_id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, RawInput, vec2};

    struct Rig {
        ctx: egui::Context,
        app: App,
        time: f64,
        size: egui::Vec2,
    }

    impl Rig {
        fn new() -> Rig {
            let ctx = egui::Context::default();
            crate::style::apply(&ctx);
            let mut rig = Rig { ctx, app: App::new(true, None), time: 0.0, size: vec2(1200.0, 1600.0) };
            rig.frame(vec![]);
            rig
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 0.016;
            let input =
                RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), events, time: Some(self.time), ..Default::default() };
            let app = &mut self.app;
            let mut out = self.ctx.run_ui(input, |ui| update(app, ui.ctx()));
            out.textures_delta.clear();
        }

        fn open_window(&mut self) {
            open(&self.ctx, &self.app.preferences);
            // Two frames: the auto-sized card settles its height on the second one.
            self.frame(vec![]);
            self.frame(vec![]);
        }

        fn click(&mut self, id: &str) {
            let p = hit(&self.ctx, id).unwrap_or_else(|| panic!("{id} was laid out")).center();
            self.frame(vec![Event::PointerMoved(p)]);
            for pressed in [true, false] {
                self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]);
            }
            self.frame(vec![]);
        }
    }

    #[test]
    fn window_opens_shows_every_control_and_closes_without_a_shortcut() {
        let mut r = Rig::new();
        r.open_window();
        assert!(is_open(&r.ctx));
        for id in CONTROL_IDS {
            assert!(hit(&r.ctx, id).is_some(), "{id} is laid out and clickable");
        }
        r.click("settings.lang.en");
        assert_eq!(r.app.preferences.lang, "en");
        assert_eq!(crate::prefs::current_lang(), "en");
        assert_eq!(tr("settings.title"), "Preferences");
        r.click("settings.close");
        assert!(!is_open(&r.ctx));

        r.open_window();
        let units = r.app.preferences.units.clone();
        r.click("settings.units.cm");
        assert_eq!(r.app.preferences.units, "cm");
        r.click("settings.cancel");
        assert!(!is_open(&r.ctx));
        assert_eq!(r.app.preferences.units, units, "Cancel restores the opening snapshot");
    }

    #[test]
    fn every_control_changes_a_value_and_reset_restores_defaults() {
        let mut r = Rig::new();
        r.open_window();
        r.click("settings.lang.zh-Hans");
        r.click("settings.units.in");
        r.click("settings.grid.off");
        r.click("settings.snap.grid.off");
        r.click("settings.snap.vertex.off");
        r.click("settings.nav.cad");
        r.click("settings.export.format.obj");
        r.click("settings.export.units.cm");
        r.click("settings.export.up.y");
        r.frame(vec![]);
        assert_eq!(r.app.preferences.lang, "zh-Hans");
        assert_eq!(r.app.preferences.units, "in");
        assert!(!r.app.preferences.grid && !r.app.preferences.snap_grid && !r.app.preferences.snap_vertex);
        assert_eq!(r.app.preferences.navigation(), Navigation::Cad);
        assert_eq!(r.app.preferences.export_format(), crate::export_formats::Format::Obj);
        assert_eq!(r.app.preferences.export_units(), crate::export_formats::Units::Cm);
        assert_eq!(r.app.preferences.export_up(), crate::export_job::Up::Y);

        r.click("settings.reset");
        assert_eq!(r.app.preferences, Preferences::default());
        assert_eq!(crate::prefs::current_lang(), "ja", "Reset also restores the default language");
        assert!(is_open(&r.ctx), "Reset keeps the window open for further changes");
    }

    struct Memory(std::collections::HashMap<String, String>);
    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn choices_survive_a_second_app_through_eframe_storage() {
        let mut r = Rig::new();
        r.open_window();
        r.click("settings.lang.en");
        r.click("settings.units.cm");
        r.click("settings.nav.cad");
        r.click("settings.autosave");
        let expected = r.app.preferences.clone();
        assert_eq!(expected.lang, "en");
        assert_eq!(expected.units, "cm");
        assert_eq!(expected.navigation(), Navigation::Cad);
        assert!(expected.autosave_minutes > crate::prefs::DEFAULT_AUTOSAVE_MINUTES);

        let mut memory = Memory(std::collections::HashMap::new());
        eframe::App::save(&mut r.app, &mut memory);
        assert!(eframe::Storage::get_string(&memory, crate::prefs::PREFS_KEY).is_some());

        let mut second = App::new(true, None);
        second.load_preferences(Some(&memory));
        assert_eq!(second.preferences, expected, "a new session reads the same values");
        assert_eq!(crate::prefs::current_lang(), "en");
        assert_eq!(tr("settings.title"), "Preferences");
    }

    #[test]
    fn sliders_are_pointer_reachable_and_change_their_values() {
        let mut r = Rig::new();
        r.open_window();
        let start = r.app.preferences.autosave_minutes;
        let rect = hit(&r.ctx, "settings.autosave").expect("autosave slider");
        let p = rect.center();
        r.frame(vec![Event::PointerMoved(p)]);
        r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        r.frame(vec![]);
        assert!(r.app.preferences.autosave_minutes > start, "clicking the middle raises autosave minutes: {}", r.app.preferences.autosave_minutes);

        let rect = hit(&r.ctx, "settings.grid.size").expect("grid size slider");
        let p = Pos2::new(rect.left() + 8.0, rect.center().y);
        r.frame(vec![Event::PointerMoved(p)]);
        r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        r.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        r.frame(vec![]);
        assert_eq!(r.app.preferences.grid_size_mm, MIN_GRID_MM);
    }
}
