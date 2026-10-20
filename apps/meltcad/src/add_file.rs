//! File > "Add file…": another design is put into this one, inside a new group that is "Melt: on",
//! so the two melt together. "Open" replaces the whole scene; this keeps what is already there.

use std::path::Path;

use crate::app::App;
use crate::model::{Group, MAX_DEPTH, MAX_GROUPS, MAX_SHAPES, Pick, Scene};
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
pub fn merge_into(scene: &mut Scene, mut other: Scene, name: &str) -> Result<usize, String> {
    if other.groups.is_empty() {
        return Err("That file has nothing in it".into());
    }
    let wrap = other.groups.iter().all(|g| g.depth < MAX_DEPTH);
    if scene.groups.len() + other.groups.len() + usize::from(wrap) > MAX_GROUPS {
        return Err(format!("Up to {MAX_GROUPS} groups"));
    }
    if scene.shape_count() + other.shape_count() > MAX_SHAPES {
        return Err(format!("Up to {MAX_SHAPES} shapes"));
    }
    crate::mesh::can_add(scene, other.groups.iter().flat_map(|g| g.shapes.iter()))?;
    shift_uids(&mut other, max_uid(scene));
    let at = scene.groups.len();
    if wrap {
        let title: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
        let mut parent = Group::new(if title.trim().is_empty() { "Added file".to_string() } else { title });
        parent.blend = ADDED_BLEND;
        scene.groups.push(parent);
        for mut g in other.groups {
            g.depth += 1;
            scene.groups.push(g);
        }
    } else {
        // nested too deep to be wrapped: the first top-level group melts into the scene instead
        let first_top = other.groups.iter().position(|g| g.depth == 0);
        if let Some(g) = first_top.and_then(|i| other.groups.get_mut(i)) {
            g.blend = g.blend.max(ADDED_BLEND);
        }
        scene.groups.append(&mut other.groups);
    }
    scene.normalize_depths();
    Ok(at)
}

/// The scene file at `path`, added to `scene`.
pub fn add_file(scene: &mut Scene, path: &Path) -> Result<usize, String> {
    let (other, _notes) = crate::recovery::read_project(path)?;
    let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    merge_into(scene, other, &name)
}

impl App {
    /// File > Add file…: pick a design and add it to this one.
    pub(crate) fn add_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("Melt 3D", &["meltcad", "yura", "json"]).pick_file() else { return };
        self.add_file_path(ctx, &path);
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
