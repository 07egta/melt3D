//! Copy, cut and paste. A copied shape or group goes to the system clipboard as JSON text, so it
//! can be pasted into another MELTCAD window (or kept in a text file). A whole `.yura` scene
//! pasted as text is added as new groups.

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Pick, Scene, Shape};

/// How far a pasted copy sits from the original, so it can be seen.
const STEP: f32 = 8.0;

#[derive(Serialize, Deserialize)]
struct Clip {
    yuracad_clip: u32,
    /// The copied group (all of it when `whole`, else only its first shape matters).
    group: Group,
    whole: bool,
}

/// The selection as clipboard text (None when nothing is selected).
pub fn copy_text(app: &App) -> Option<String> {
    let clip = match app.sel? {
        Pick::Shape(g, s) => {
            let shape = app.scene.shape(g, s)?.clone();
            let mut group = Group::new("");
            group.shapes.push(shape);
            Clip { yuracad_clip: 1, group, whole: false }
        }
        Pick::Group(g) => Clip { yuracad_clip: 1, group: app.scene.groups.get(g)?.clone(), whole: true },
    };
    serde_json::to_string(&clip).ok()
}

/// Put the text into the scene. Returns what was added ("" when the text is not something MELTCAD knows).
pub fn paste_text(app: &mut App, text: &str) -> Result<String, String> {
    let text = text.trim();
    let _guard = crate::mesh::LoadGuard::begin();
    if let Ok(clip) = serde_json::from_str::<Clip>(text) {
        if clip.yuracad_clip != 1 {
            return Err("This copy is from a newer version".into());
        }
        let mut sc = Scene { groups: vec![clip.group], ..Scene::default() };
        sc.sanitize();
        let group = sc.groups.pop().ok_or("It was empty")?;
        return if clip.whole { add_group(app, group, true) } else { add_shapes(app, group.shapes) };
    }
    // a whole scene file pasted as text: its groups are added
    if let Ok(sc) = Scene::from_json(text) {
        let mut n = 0;
        for g in sc.groups {
            add_group(app, g, false)?;
            n += 1;
        }
        return Ok(format!("Pasted {n} groups"));
    }
    Err("Nothing to paste".into())
}

fn shifted(shapes: &mut [Shape], d: f32) {
    for s in shapes {
        s.pos[0] += d;
        s.pos[1] += d;
    }
}

fn add_shapes(app: &mut App, mut shapes: Vec<Shape>) -> Result<String, String> {
    if shapes.is_empty() {
        return Err("It was empty".into());
    }
    if app.scene.shape_count() + shapes.len() > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    if app.scene.groups.is_empty() {
        app.scene.groups.push(Group::new("Group 1"));
    }
    let g = match app.sel {
        Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) if g < app.scene.groups.len() => g,
        _ => app.scene.groups.len() - 1,
    };
    crate::mesh::can_add(&app.scene, shapes.iter())?;
    shifted(&mut shapes, STEP);
    for s in &mut shapes {
        s.name = format!("{} copy", s.name.trim_end_matches(" copy"));
    }
    let group = &mut app.scene.groups[g];
    group.shapes.extend(shapes);
    app.sel = Some(Pick::Shape(g, group.shapes.len() - 1));
    Ok("Pasted".into())
}

fn add_group(app: &mut App, mut group: Group, offset: bool) -> Result<String, String> {
    if app.scene.groups.len() >= MAX_GROUPS {
        return Err(format!("Up to {MAX_GROUPS} groups"));
    }
    if app.scene.shape_count() + group.shapes.len() > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    crate::mesh::can_add(&app.scene, group.shapes.iter())?;
    if offset {
        shifted(&mut group.shapes, STEP);
    }
    let at = match app.sel {
        Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) => app.scene.subtree_end(g).min(app.scene.groups.len()),
        None => app.scene.groups.len(),
    };
    let mut group = group;
    group.depth = 0;
    app.scene.groups.insert(at, group);
    app.scene.normalize_depths();
    app.sel = Some(Pick::Group(at));
    Ok("Pasted".into())
}

impl App {
    /// Copy the selection (to the system clipboard and to the app's own copy for the menu).
    pub fn copy(&mut self, ctx: &egui::Context) {
        match copy_text(self) {
            Some(t) => {
                ctx.copy_text(t.clone());
                self.clipboard = Some(t);
                self.say(ctx, "Copied");
            }
            None => self.say(ctx, "Select something to copy"),
        }
    }

    pub fn cut(&mut self, ctx: &egui::Context) {
        if self.sel.is_some() {
            self.copy(ctx);
            self.delete_selected();
        }
    }

    /// Paste the given text (a Ctrl+V), or the app's own copy when `text` is None (the menu).
    pub fn paste(&mut self, ctx: &egui::Context, text: Option<String>) {
        let Some(t) = text.or_else(|| self.clipboard.clone()) else {
            self.say(ctx, "Nothing copied yet");
            return;
        };
        match paste_text(self, &t) {
            Ok(m) => self.say(ctx, m),
            Err(e) => self.say(ctx, e),
        }
    }

    /// Copy / cut / paste events from the keyboard (egui turns Ctrl+C, X and V into these).
    pub fn clipboard_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Copy => self.copy(ctx),
                egui::Event::Cut => self.cut(ctx),
                egui::Event::Paste(t) => self.paste(ctx, Some(t)),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    #[test]
    fn a_shape_copy_pastes_beside_the_original() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        let t = copy_text(&app).unwrap_or_default();
        assert!(paste_text(&mut app, &t).is_ok());
        assert_eq!(app.scene.shape_count(), 2);
        let (a, b) = (app.scene.shape(0, 0).map(|s| s.pos), app.scene.shape(0, 1).map(|s| s.pos));
        assert_eq!(b.map(|p| p[0] - STEP), a.map(|p| p[0]));
        assert_eq!(app.sel, Some(Pick::Shape(0, 1)));
    }

    #[test]
    fn a_group_copy_pastes_as_a_new_group_and_limits_hold() {
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Group(0));
        let t = copy_text(&app).unwrap_or_default();
        assert!(paste_text(&mut app, &t).is_ok());
        assert_eq!(app.scene.groups.len(), 2);
        assert_eq!(app.sel, Some(Pick::Group(1)));
        for _ in 0..200 {
            let _ = paste_text(&mut app, &t);
        }
        assert!(app.scene.groups.len() <= MAX_GROUPS && app.scene.shape_count() <= MAX_SHAPES);
    }

    #[test]
    fn a_pasted_scene_file_adds_its_groups_and_junk_is_refused() {
        let mut app = App::new(true, None);
        let mut other = Scene::default();
        other.groups[0].shapes[0].kind = Kind::Box;
        assert!(paste_text(&mut app, &other.to_json()).is_ok());
        assert_eq!(app.scene.groups.len(), 2);
        assert!(paste_text(&mut app, "hello").is_err());
        assert!(paste_text(&mut app, "{\"yuracad_clip\":9,\"group\":{},\"whole\":true}").is_err());
        // hostile numbers are made safe
        let bad = r#"{"yuracad_clip":1,"whole":false,"group":{"name":"","op":"Add","blend":0,"visible":true,"shapes":[{"name":"s","kind":"Box","size":[1e30,1,1],"round":0,"pos":[1e30,0,0],"rot":[0,0,0],"scale":[1,1,1],"op":"Add","blend":1,"material":{"color":[1,1,1],"roughness":0.5,"metallic":0},"visible":true,"outline":[]}]}}"#;
        assert!(paste_text(&mut app, bad).is_ok());
        let s = app.scene.groups[0].shapes.last().cloned();
        assert!(s.is_some_and(|s| s.size[0] <= 2000.0 && s.pos[0] <= 5000.0));
    }

    #[test]
    fn cut_removes_and_the_menu_can_paste_it_back() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.add_shape(&ctx, Kind::Box);
        app.cut(&ctx);
        assert_eq!(app.scene.shape_count(), 1);
        app.paste(&ctx, None);
        assert_eq!(app.scene.shape_count(), 2);
    }
}
