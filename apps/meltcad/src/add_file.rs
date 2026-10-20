//! File > "Add file…": another design is put into this one, inside a new group that is "Melt: on",
//! so the two melt together. "Open" replaces the whole scene; this keeps what is already there.

use std::path::Path;

use crate::app::App;
use crate::model::{Group, MAX_DEPTH, MAX_GROUPS, MAX_SHAPES, Pick, Piece, Scene};
use crate::pattern::{Pattern, Source};

/// How far an added file melts into what was there before (mm): the usual "Melt: on" of a group.
const ADDED_BLEND: f32 = 12.0;

/// The longest name the new group takes from a file name.
const MAX_NAME_CHARS: usize = 60;

fn max_uid(sc: &Scene) -> u32 {
    sc.groups.iter().flat_map(|g| g.shapes.iter().map(|s| s.uid).chain(std::iter::once(g.uid))).max().unwrap_or(0)
}

/// Give every number the added file links things by (copies, patterns) a place of its own, so
/// nothing it points at can be a group or shape of the scene it is added to.
fn shift_uids(sc: &mut Scene, by: u32) {
    let bump = |u: &mut u32| {
        if *u != 0 {
            *u = u.saturating_add(by);
        }
    };
    for g in &mut sc.groups {
        bump(&mut g.uid);
        if let Some(i) = &mut g.instance_of {
            bump(i);
        }
        match &mut g.pattern {
            Some(Pattern::Path { source, .. }) => bump(source),
            Some(Pattern::Geometry { source: Source::Shape(u) | Source::Group(u), .. }) => bump(u),
            _ => {}
        }
        for s in &mut g.shapes {
            bump(&mut s.uid);
        }
    }
}

/// Put `other` at the end of `scene`. It goes inside one new group (named `name`) with "Melt: on",
/// so the whole file melts into what was there; its own groups keep their order and nesting.
/// Returns the index of the first group added.
#[cfg(test)]
pub fn merge_into(scene: &mut Scene, other: Scene, name: &str) -> Result<usize, String> {
    let at = scene.groups.len();
    merge_at(scene, other, name, None, at, 0)
}

/// `merge_into` at place `at` of the list, with the new group at nesting `depth`. The new group is a
/// piece of the big file: it remembers `path`, so it can be read again or saved back.
fn merge_at(scene: &mut Scene, mut other: Scene, name: &str, path: Option<&Path>, at: usize, depth: u8) -> Result<usize, String> {
    if other.groups.is_empty() {
        return Err("That file has nothing in it".into());
    }
    let wrap = other.groups.iter().all(|g| u16::from(g.depth) + u16::from(depth) < u16::from(MAX_DEPTH));
    if scene.groups.len() + other.groups.len() + usize::from(wrap) > MAX_GROUPS {
        return Err(format!("Up to {MAX_GROUPS} groups"));
    }
    if scene.shape_count() + other.shape_count() > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    crate::mesh::can_add(scene, other.groups.iter().flat_map(|g| g.shapes.iter()))?;
    shift_uids(&mut other, max_uid(scene));
    let at = at.min(scene.groups.len());
    let mut block: Vec<Group> = Vec::new();
    if wrap {
        let title: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
        let mut parent = Group::new(if title.trim().is_empty() { "Added file".to_string() } else { title.clone() });
        parent.blend = ADDED_BLEND;
        parent.depth = depth;
        parent.piece = Some(Piece { file: title, path: path.map(|p| p.to_string_lossy().into_owned()) });
        block.push(parent);
        for mut g in other.groups {
            g.depth += depth + 1;
            block.push(g);
        }
    } else {
        // nested too deep to be wrapped: the first top-level group melts into the scene instead
        let first_top = other.groups.iter().position(|g| g.depth == 0);
        if let Some(g) = first_top.and_then(|i| other.groups.get_mut(i)) {
            g.blend = g.blend.max(ADDED_BLEND);
        }
        for mut g in other.groups {
            g.depth = g.depth.saturating_add(depth);
            block.push(g);
        }
    }
    scene.groups.splice(at..at, block);
    scene.normalize_depths();
    Ok(at)
}

/// The scene file at `path`, added to `scene`.
pub fn add_file(scene: &mut Scene, path: &Path) -> Result<usize, String> {
    let (other, _notes) = crate::recovery::read_project(path)?;
    let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let at = scene.groups.len();
    merge_at(scene, other, &name, Some(path), at, 0)
}

/// Read the file of piece `gi` again and put it in the piece's place. The piece keeps its melt
/// amount, its +/- and whether it is shown. Nothing changes when the file cannot be read or does not fit.
pub fn reload_piece(scene: &mut Scene, gi: usize) -> Result<(), String> {
    let old = scene.groups.get(gi).cloned().ok_or("No such group")?;
    let piece = old.piece.clone().ok_or("This group is not a piece of a file")?;
    let path = piece.path.clone().ok_or("Where this piece came from is not known. Use Save as file… first")?;
    let (other, _notes) = crate::recovery::read_project(Path::new(&path))?;
    let end = scene.subtree_end(gi);
    let removed: Vec<Group> = scene.groups.drain(gi..end).collect();
    match merge_at(scene, other, &piece.file, Some(Path::new(&path)), gi, old.depth) {
        Ok(at) => {
            if let Some(g) = scene.groups.get_mut(at) {
                g.name = old.name;
                g.blend = old.blend;
                g.visible = old.visible;
                g.op = old.op;
                g.xform = old.xform;
            }
            Ok(())
        }
        Err(e) => {
            scene.groups.splice(gi..gi, removed);
            Err(e)
        }
    }
}

/// Write piece `gi` (what is inside it) into its own file at `path`, and remember that file for it.
pub fn save_piece(scene: &mut Scene, gi: usize, path: &Path) -> Result<(), String> {
    let g = scene.groups.get(gi).ok_or("No such group")?;
    if g.piece.is_none() {
        return Err("This group is not a piece of a file".into());
    }
    let end = scene.subtree_end(gi);
    // When shapes were added directly to the Piece wrapper, preserve that
    // wrapper as the file's root. Without it, Save as file silently loses them.
    // Empty wrappers retain the original layout (their children become roots).
    let include_parent = !g.shapes.is_empty();
    let first = if include_parent { gi } else { gi + 1 };
    let relative_depth = u16::from(g.depth) + u16::from(!include_parent);
    let mut inside: Vec<Group> = scene
        .groups
        .get(first..end)
        .unwrap_or_default()
        .iter()
        .cloned()
        .map(|mut x| {
            x.depth = u16::from(x.depth).saturating_sub(relative_depth) as u8;
            x
        })
        .collect();
    if include_parent {
        if let Some(parent) = inside.first_mut() {
            parent.piece = None; // The exported file must not refer back to itself.
        }
    }
    let file = Scene { groups: inside, melt: scene.melt, ..Scene::default() };
    crate::recovery::save_project(path, &file)?;
    if let Some(p) = scene.groups.get_mut(gi).and_then(|g| g.piece.as_mut()) {
        p.file = path.file_stem().map(|n| n.to_string_lossy().chars().take(crate::model::MAX_PIECE_FILE).collect()).unwrap_or_default();
        p.path = Some(path.to_string_lossy().into_owned());
    }
    Ok(())
}

impl App {
    /// File > Add file…: pick a design and add it to this one.
    pub(crate) fn add_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("Melt 3D", &["meltcad", "yura", "json"]).pick_file() else { return };
        self.add_file_path(ctx, &path);
    }

    /// In the group's panel: a piece of the big file says where it came from, and can read that
    /// file again or be saved as a file of its own.
    pub(crate) fn piece_card(&mut self, ui: &mut egui::Ui, g: usize) {
        let Some(piece) = self.scene.groups.get(g).and_then(|x| x.piece.clone()) else { return };
        let ctx = ui.ctx().clone();
        ui.label(egui::RichText::new(format!("Piece of the big file: {}", piece.file)).size(12.0).color(crate::style::DIM))
            .on_hover_text("A whole file you added. It melts with the other pieces and shapes of this scene");
        let free = !self.scene_structure_locked();
        ui.horizontal_wrapped(|ui| {
            if crate::style::pill_enabled(ui, "Reload", false, free && piece.path.is_some())
                .on_hover_text("Read the file again and replace this piece with it")
                .clicked()
            {
                match reload_piece(&mut self.scene, g) {
                    Ok(()) => {
                        self.sel = Some(Pick::Group(g));
                        self.say(&ctx, "Read the file again");
                    }
                    Err(e) => self.say(&ctx, format!("Could not reload: {e}")),
                }
            }
            if crate::style::pill_enabled(ui, "Save as file…", false, free).on_hover_text("Write this piece into a file of its own").clicked()
                && let Some(path) =
                    rfd::FileDialog::new().add_filter("Melt 3D", &["meltcad"]).set_file_name(format!("{}.meltcad", piece.file)).save_file()
            {
                match save_piece(&mut self.scene, g, &path) {
                    Ok(()) => self.say(&ctx, "Saved the piece as a file"),
                    Err(e) => self.say(&ctx, format!("Could not save: {e}")),
                }
            }
        });
    }

    pub(crate) fn add_file_path(&mut self, ctx: &egui::Context, path: &Path) {
        if !self.allow_scene_structure_change() {
            return;
        }
        match add_file(&mut self.scene, path) {
            Ok(at) => {
                self.sel = Some(Pick::Group(at));
                self.frame_view(false);
                self.say(ctx, "Added the file. It melts with what was already here");
            }
            Err(e) => self.say(ctx, format!("Could not add the file: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material, Shape};
    use crate::sdf::Field;

    fn ball(x: f32) -> Shape {
        let mut s = Shape::new(Kind::Sphere, [x, 0.0, 0.0], Material::default());
        s.blend = 0.0;
        s
    }

    fn scene_with(groups: Vec<Group>) -> Scene {
        Scene { groups, ..Scene::default() }
    }

    fn group(name: &str, depth: u8, shapes: Vec<Shape>) -> Group {
        let mut g = Group::new(name);
        g.depth = depth;
        g.shapes = shapes;
        g
    }

    #[test]
    fn an_added_file_goes_in_a_group_that_melts_and_keeps_its_nesting() {
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(-22.0)])]);
        let other = scene_with(vec![group("a", 0, vec![ball(22.0)]), group("b", 1, vec![ball(90.0)])]);
        let at = merge_into(&mut mine, other, "friend").unwrap();
        assert_eq!(at, 1);
        let depths: Vec<u8> = mine.groups.iter().map(|g| g.depth).collect();
        assert_eq!(depths, [0, 0, 1, 2]);
        assert_eq!(mine.groups[1].name, "friend");
        assert!(mine.groups[1].blend > 0.0, "Melt: on");
        // two balls 44 mm apart (radius 20) leave a 4 mm gap alone, and they join once melted
        let mut apart = mine.clone();
        apart.groups[1].blend = 0.0;
        let alone = Field::new(&apart).distance([0.0, 0.0, 0.0]);
        assert!(
            alone > 0.0,
            "without melting there is a gap: {alone} amount {} shapes {:?}",
            apart.melt.amount,
            apart.groups.iter().map(|g| (g.depth, g.blend, g.shapes.len())).collect::<Vec<_>>()
        );
        mine.melt.amount = 1.0;
        let gap = Field::new(&mine).distance([0.0, 0.0, 0.0]);
        assert!(gap < 0.0, "the added file melts into the scene: {gap}");
    }

    #[test]
    fn links_in_the_added_file_never_point_into_the_scene() {
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        mine.groups[0].uid = 1;
        mine.groups[0].shapes[0].uid = 2;
        let mut theirs = group("t", 0, vec![ball(50.0)]);
        theirs.uid = 1;
        let mut copy = group("c", 0, vec![]);
        copy.instance_of = Some(1);
        let other = scene_with(vec![theirs, copy]);
        merge_into(&mut mine, other, "x").unwrap();
        let uids: Vec<u32> = mine.groups.iter().map(|g| g.uid).filter(|u| *u != 0).collect();
        assert_eq!(uids.len(), 2);
        assert_ne!(uids[0], uids[1]);
        let copy = mine.groups.iter().find(|g| g.instance_of.is_some()).unwrap();
        assert_eq!(copy.instance_of, mine.groups.iter().find(|g| g.name == "t").map(|g| g.uid));
    }

    #[test]
    fn limits_and_empty_files_are_refused_and_change_nothing() {
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        let before = mine.clone();
        assert!(merge_into(&mut mine, scene_with(vec![]), "e").is_err());
        let many = scene_with((0..MAX_GROUPS).map(|i| group(&format!("g{i}"), 0, vec![])).collect());
        assert!(merge_into(&mut mine, many, "m").is_err());
        assert_eq!(mine, before);
    }

    #[test]
    fn a_file_nested_too_deep_to_wrap_still_melts_at_its_first_top_group() {
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        let deep = scene_with(vec![group("a", 0, vec![]), group("b", 1, vec![]), group("c", 2, vec![]), group("d", MAX_DEPTH, vec![ball(30.0)])]);
        let at = merge_into(&mut mine, deep, "deep").unwrap();
        assert_eq!(mine.groups[at].name, "a");
        assert!(mine.groups[at].blend > 0.0);
        assert_eq!(mine.groups.len(), 5);
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("meltcad-piece-{}-{name}.meltcad", std::process::id()))
    }

    #[test]
    fn an_added_file_is_a_piece_that_reloads_and_saves_back() {
        let first = temp("first");
        crate::recovery::save_project(&first, &scene_with(vec![group("t", 0, vec![ball(30.0)])])).unwrap();
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        let at = add_file(&mut mine, &first).unwrap();
        let piece = mine.groups[at].piece.clone().expect("a piece of the big file");
        assert!(piece.path.as_deref().is_some_and(|p| p.ends_with("first.meltcad")));
        // the user set this piece apart and renamed it: a reload keeps that, and takes the new contents
        mine.groups[at].blend = 33.0;
        mine.groups[at].name = "my friend".into();
        crate::recovery::save_project(&first, &scene_with(vec![group("t", 0, vec![ball(30.0), ball(60.0)])])).unwrap();
        reload_piece(&mut mine, at).unwrap();
        assert_eq!(mine.shape_count(), 3);
        assert_eq!((mine.groups[at].blend, mine.groups[at].name.as_str()), (33.0, "my friend"));
        assert!(mine.groups[at].piece.is_some());
        // saving the piece writes only what is inside it, back at the top level
        let second = temp("second");
        save_piece(&mut mine, at, &second).unwrap();
        let (back, _) = crate::recovery::read_project(&second).unwrap();
        assert_eq!(back.shape_count(), 2);
        assert!(back.groups.iter().all(|g| g.depth == 0 && g.piece.is_none()));
        assert!(mine.groups[at].piece.as_ref().and_then(|p| p.path.as_deref()).is_some_and(|p| p.ends_with("second.meltcad")));
        // the piece survives a save and a load of the big file
        let (loaded, _) = Scene::from_json_notes(&mine.to_json()).unwrap();
        assert_eq!(loaded.groups[at].piece, mine.groups[at].piece);
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
    }

    #[test]
    fn saving_a_piece_preserves_shapes_added_directly_to_the_wrapper() {
        let original = temp("piece-original");
        let output = temp("piece-wrapper-shapes");
        crate::recovery::save_project(&original, &scene_with(vec![group("inside", 0, vec![ball(30.0)])])).unwrap();
        let mut scene = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        let at = add_file(&mut scene, &original).unwrap();
        let mut box_shape = Shape::new(Kind::Box, [70.0, 0.0, 10.0], Material::default());
        box_shape.blend = 0.0;
        scene.groups[at].shapes.push(box_shape);
        assert_eq!(scene.groups[at].depth, 0);
        assert_eq!(scene.shape_count(), 3);
        save_piece(&mut scene, at, &output).unwrap();
        let (saved, _) = crate::recovery::read_project(&output).unwrap();
        assert_eq!(saved.shape_count(), 2, "the Piece's own box must survive export");
        assert_eq!(saved.groups[0].shapes[0].kind, Kind::Box);
        assert_eq!(saved.groups[0].depth, 0);
        assert!(saved.groups[0].piece.is_none());
        assert_eq!(saved.groups[1].depth, 1);
        assert_eq!(saved.groups[1].shapes[0].kind, Kind::Sphere);
        let _ = std::fs::remove_file(original);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn a_failed_reload_leaves_the_scene_as_it_was() {
        let gone = temp("gone");
        crate::recovery::save_project(&gone, &scene_with(vec![group("t", 0, vec![ball(30.0)])])).unwrap();
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        let at = add_file(&mut mine, &gone).unwrap();
        let before = mine.clone();
        let _ = std::fs::remove_file(&gone);
        assert!(reload_piece(&mut mine, at).is_err());
        assert_eq!(mine, before);
        assert!(reload_piece(&mut mine, 0).is_err(), "a plain group is not a piece");
        assert!(reload_piece(&mut mine, 99).is_err());
        assert!(save_piece(&mut mine, 0, &temp("never")).is_err());
    }

    #[test]
    fn a_saved_file_can_be_added_and_a_missing_one_is_an_error() {
        let path = std::env::temp_dir().join(format!("meltcad-add-file-{}.meltcad", std::process::id()));
        let theirs = scene_with(vec![group("t", 0, vec![ball(30.0)])]);
        crate::recovery::save_project(&path, &theirs).unwrap();
        let mut mine = scene_with(vec![group("mine", 0, vec![ball(0.0)])]);
        add_file(&mut mine, &path).unwrap();
        assert_eq!(mine.shape_count(), 2);
        assert!(mine.groups.iter().any(|g| g.name.starts_with("meltcad-add-file")));
        let _ = std::fs::remove_file(&path);
        assert!(add_file(&mut mine, &path).is_err());
    }
}
