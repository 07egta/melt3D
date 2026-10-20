//! Copy, cut and paste. A copied shape or group goes to the system clipboard as JSON text, so it
//! can be pasted into another MELTCAD window (or kept in a text file). A whole `.yura` scene
//! pasted as text is added as new groups.

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Pick, Scene, Shape};

/// A pasted copy sits exactly where the original was (no shift).
const STEP: f32 = 0.0;

#[derive(Serialize, Deserialize)]
struct Clip {
    yuracad_clip: u32,
    /// The copied group (all of it when `whole`, else only its first shape matters).
    group: Group,
    whole: bool,
    /// The groups nested inside `group`, in order (with the depths they had): a copied group
    /// takes its whole outline along, like Duplicate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    inner: Vec<Group>,
}

/// The selection as clipboard text (None when nothing is selected).
pub fn copy_text(app: &App) -> Option<String> {
    let clip = match app.sel? {
        Pick::Shape(g, s) => {
            let shape = app.scene.shape(g, s)?.clone();
            let mut group = Group::new("");
            group.shapes.push(shape);
            Clip { yuracad_clip: 1, group, whole: false, inner: Vec::new() }
        }
        Pick::Group(g) => {
            let end = app.scene.subtree_end(g);
            let inner = app.scene.groups.get(g + 1..end).map(<[Group]>::to_vec).unwrap_or_default();
            Clip { yuracad_clip: 1, group: app.scene.groups.get(g)?.clone(), whole: true, inner }
        }
    };
    serde_json::to_string(&clip).ok()
}

/// Put the text into the scene. Returns what was added ("" when the text is not something MELTCAD knows).
pub fn paste_text(app: &mut App, text: &str) -> Result<String, String> {
    if !app.allow_scene_structure_change() {
        return Err(app.sk.msg.clone());
    }
    let text = text.trim();
    let _guard = crate::mesh::LoadGuard::begin();
    if let Ok(clip) = serde_json::from_str::<Clip>(text) {
        if clip.yuracad_clip != 1 {
            return Err("This copy is from a newer version".into());
        }
        // the top group stands at depth 0 and what was inside it keeps its place below it
        let mut top = clip.group;
        let base = top.depth;
        top.depth = 0;
        let mut groups = vec![top];
        if clip.whole {
            groups.extend(clip.inner.into_iter().map(|mut g| {
                g.depth = g.depth.saturating_sub(base).max(1);
                g
            }));
        }
        let mut sc = Scene { groups, ..Scene::default() };
        sc.sanitize();
        if !clip.whole {
            let group = sc.groups.into_iter().next().ok_or("It was empty")?;
            return add_shapes(app, group.shapes);
        }
        return add_block(app, sc.groups, "Pasted");
    }
    // a whole scene file pasted as text: its groups are added
    if let Ok(sc) = Scene::from_json(text) {
        let n = sc.groups.len();
        return add_block(app, sc.groups, &format!("Pasted {n} groups"));
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
    if app.scene.groups[g].is_linked() {
        return Err("This is a linked copy. Select its source or Unlink it before pasting shapes".into());
    }
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

/// Put a block of groups (a group with the groups nested in it, or a whole scene) into the scene
/// after the selection, keeping how they nest. All of it goes in, or none of it does.
fn add_block(app: &mut App, mut block: Vec<Group>, done: &str) -> Result<String, String> {
    if block.is_empty() {
        return Err("It was empty".into());
    }
    if app.scene.groups.len() + block.len() > MAX_GROUPS {
        return Err(format!("Up to {MAX_GROUPS} groups"));
    }
    if app.scene.shape_count() + block.iter().map(|g| g.shapes.len()).sum::<usize>() > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    crate::mesh::can_add(&app.scene, block.iter().flat_map(|g| g.shapes.iter()))?;
    // Copied scenes may reuse IDs from the destination. Rebase group IDs,
    // shape IDs, link sources and pattern references *before* splicing so
    // linked copies cannot silently attach to pre-existing geometry.
    let mut incoming = Scene { groups: block, ..Scene::default() };
    let by = crate::add_file::max_uid(&app.scene);
    if by.checked_add(crate::add_file::max_uid(&incoming)).is_none() {
        return Err("Too many identifiers to paste safely".into());
    }
    crate::add_file::shift_uids(&mut incoming, by);
    block = incoming.groups;
    let base = block.first().map_or(0, |g| g.depth);
    for g in &mut block {
        g.depth = g.depth.saturating_sub(base);
    }
    let at = match app.sel {
        Some(Pick::Shape(g, _)) | Some(Pick::Group(g)) => app.scene.subtree_end(g).min(app.scene.groups.len()),
        None => app.scene.groups.len(),
    };
    app.scene.groups.splice(at..at, block);
    app.scene.normalize_depths();
    app.sel = Some(Pick::Group(at));
    Ok(done.into())
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
        if !self.allow_scene_structure_change() {
            return;
        }
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

    fn nested_app() -> App {
        let mut app = App::new(true, None);
        let mut child = Group::new("child");
        child.depth = 1;
        child.shapes.push(app.scene.groups[0].shapes[0].clone());
        app.scene.groups.push(child);
        app
    }

    #[test]
    fn copying_a_parent_group_takes_its_children_along() {
        let mut app = nested_app();
        app.sel = Some(Pick::Group(0));
        let t = copy_text(&app).unwrap_or_default();
        assert!(paste_text(&mut app, &t).is_ok());
        let depths: Vec<u8> = app.scene.groups.iter().map(|g| g.depth).collect();
        assert_eq!(depths, vec![0, 1, 0, 1], "{depths:?}");
        assert_eq!(app.scene.groups[3].name, "child");
        assert_eq!(app.sel, Some(Pick::Group(2)));
    }

    #[test]
    fn a_pasted_scene_keeps_how_its_groups_nest() {
        let mut app = App::new(true, None);
        let other = nested_app().scene;
        assert!(paste_text(&mut app, &other.to_json()).is_ok());
        let depths: Vec<u8> = app.scene.groups.iter().map(|g| g.depth).collect();
        assert_eq!(depths, vec![0, 0, 1], "{depths:?}");
    }

    #[test]
    fn pasting_a_scene_remaps_colliding_link_source_uids() {
        let mut app = App::new(true, None);
        app.scene.groups[0].name = "existing sphere".into();
        app.scene.groups[0].uid = 1;
        app.sel = None;

        let mut box_group = Group::new("incoming box");
        box_group.uid = 1;
        box_group.shapes.push(Shape::new(Kind::Box, [35.0, 0.0, 10.0], Default::default()));
        let mut link = Group::new("incoming link");
        link.instance_of = Some(1);
        let incoming = Scene { groups: vec![box_group, link], ..Scene::default() };
        assert!(paste_text(&mut app, &incoming.to_json()).is_ok());
        let source_uid = app.scene.groups[1].uid;
        assert_ne!(source_uid, app.scene.groups[0].uid);
        assert_eq!(app.scene.groups[2].instance_of, Some(source_uid));
        assert_eq!(crate::pattern::link_source(&app.scene, 2), Some(1));
        assert_eq!(app.scene.groups[1].shapes[0].kind, Kind::Box);
        assert_eq!(app.scene.groups[0].shapes[0].kind, Kind::Sphere);
        let reopened = Scene::from_json(&app.scene.to_json()).unwrap();
        assert_eq!(crate::pattern::link_source(&reopened, 2), Some(1));
    }

    #[test]
    fn pasted_shapes_cannot_disappear_inside_linked_copies() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        app.sel = Some(Pick::Shape(0, 0));
        let text = copy_text(&app).unwrap();
        app.sel = Some(Pick::Group(0));
        app.make_instance(&ctx);
        let original_count = app.scene.shape_count();
        assert!(paste_text(&mut app, &text).is_err());
        assert_eq!(app.scene.shape_count(), original_count);
    }

    #[test]
    fn a_paste_that_does_not_fit_changes_nothing() {
        let mut app = App::new(true, None);
        while app.scene.groups.len() < MAX_GROUPS - 1 {
            app.scene.groups.push(Group::new("g"));
        }
        let before = app.scene.clone();
        let other = nested_app().scene;
        assert!(paste_text(&mut app, &other.to_json()).is_err());
        assert_eq!(app.scene, before, "nothing was half-added");
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
