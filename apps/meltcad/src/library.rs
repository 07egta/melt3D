//! The model library: ready-made things (people, hands, tools, furniture, food ...) that go into
//! the scene with one click. Each category is a JSON file in `apps/meltcad/library/`; see
//! `library/README.md` for the format. The files are compiled in, so nothing is read at run time.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::model::{Group, MAX_GROUPS, MAX_SHAPES, Scene};

#[derive(Deserialize)]
struct RawCategory {
    category: String,
    #[serde(default)]
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    group: Group,
}

pub struct Item {
    pub name: String,
    /// Extra words the search matches (hiragana, English, ...).
    pub tags: Vec<String>,
    pub group: Group,
}

pub struct Category {
    pub name: String,
    pub items: Vec<Item>,
}

/// The files, in the order of the tabs.
const FILES: [&str; 15] = [
    include_str!("../library/people.json"),
    include_str!("../library/hands.json"),
    include_str!("../library/animals.json"),
    include_str!("../library/tools_a.json"),
    include_str!("../library/tools_b.json"),
    include_str!("../library/furniture.json"),
    include_str!("../library/food.json"),
    include_str!("../library/vehicles.json"),
    include_str!("../library/buildings.json"),
    include_str!("../library/nature.json"),
    include_str!("../library/print.json"),
    include_str!("../library/pip.json"),
    include_str!("../library/realistic.json"),
    include_str!("../library/toon.json"),
    include_str!("../library/town.json"),
];

/// All categories (parsed once). A damaged file is skipped, never a crash; a test checks they all parse.
pub fn all() -> &'static [Category] {
    static LIB: OnceLock<Vec<Category>> = OnceLock::new();
    LIB.get_or_init(|| FILES.iter().filter_map(|t| parse(t).ok()).filter(|c| !c.items.is_empty()).collect())
}

fn parse(text: &str) -> Result<Category, String> {
    let raw: RawCategory = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let items = raw
        .items
        .into_iter()
        .map(|i| {
            // run the group through the same safety net as a loaded file
            let mut sc = Scene { groups: vec![i.group], ..Scene::default() };
            sc.sanitize();
            let group = sc.groups.into_iter().next().unwrap_or_else(|| Group::new(i.name.clone()));
            Item { name: i.name, tags: i.tags, group }
        })
        .collect();
    Ok(Category { name: raw.category, items })
}

impl Item {
    /// Does the search text match the name or a tag (case-insensitive)?
    pub fn matches(&self, q: &str) -> bool {
        let q = q.trim().to_lowercase();
        q.is_empty() || self.name.to_lowercase().contains(&q) || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    /// The item as a scene of its own (for thumbnails and the test).
    pub fn scene(&self) -> Scene {
        Scene { groups: vec![self.group.clone()], ..Scene::default() }
    }

    /// Its footprint: the centre of its shapes in x and y (so it can be put down at a chosen spot).
    fn centre_xy(&self) -> [f32; 2] {
        let n = self.group.shapes.len().max(1) as f32;
        let (x, y) = self.group.shapes.iter().fold((0.0, 0.0), |a, s| (a.0 + s.pos[0], a.1 + s.pos[1]));
        [x / n, y / n]
    }

    /// Can it still be added to a scene with this many groups and shapes?
    pub fn fits(&self, groups: usize, shapes: usize) -> bool {
        groups < MAX_GROUPS && shapes + self.group.shapes.len() <= MAX_SHAPES
    }

    /// The group to add, moved so its middle stands at `at` (x, y); z stays (models sit on the ground).
    pub fn placed(&self, at: [f32; 2]) -> Group {
        let c = self.centre_xy();
        let mut g = self.group.clone();
        for s in &mut g.shapes {
            s.pos[0] += at[0] - c[0];
            s.pos[1] += at[1] - c[1];
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_file_parses_and_every_item_fits_a_scene() {
        for (i, t) in FILES.iter().enumerate() {
            let c = parse(t).unwrap_or_else(|e| panic!("library file {i} is broken: {e}"));
            assert!(!c.name.is_empty(), "file {i} has no category name");
            let mut seen = std::collections::HashSet::new();
            for it in &c.items {
                assert!(!it.name.is_empty());
                assert!(seen.insert(it.name.clone()), "duplicate name {} in {}", it.name, c.name);
                assert!(!it.group.shapes.is_empty(), "{} is empty", it.name);
                assert!(it.group.shapes.len() <= 40, "{} has too many shapes ({})", it.name, it.group.shapes.len());
                assert!(it.fits(0, 0));
                // every item is a plain-add group: the first shape adds
                assert_eq!(it.group.op, crate::model::Op::Add, "{}", it.name);
                assert!(it.group.shapes.iter().any(|s| s.op == crate::model::Op::Add), "{} has nothing to add", it.name);
                // what the loader would do to the numbers changes nothing
                let mut sc = it.scene();
                let before = sc.clone();
                sc.sanitize();
                assert_eq!(sc, before, "{} has numbers outside the safe ranges", it.name);
            }
        }
    }

    #[test]
    fn requested_food_models_are_real_3d_and_survive_save_and_export() {
        use crate::export_formats::Format;
        use crate::export_job::{self, Lighten, Options, Scope};
        use crate::export_mesh::Progress;
        use std::path::Path;

        let c = all().iter().find(|c| c.name == "たべもの").expect("food category");
        let wanted = [
            "りんご", "バナナ", "いちご", "食パン", "クロワッサン",
            "おにぎり", "ハンバーガー", "ピザ", "ドーナツ", "ソフトクリーム",
        ];
        for name in wanted {
            assert_eq!(c.items.iter().filter(|it| it.name == name).count(), 1, "duplicate or missing {name}");
            let it = c.items.iter().find(|it| it.name == name).expect("food item");
            assert!(!it.group.shapes.is_empty() && it.group.shapes.len() <= 24, "{name}");
            let scene = it.scene();
            let back = Scene::from_json(&scene.to_json()).expect("food scene reload");
            assert_eq!(back, scene, "{name} failed save/reload");
            assert!(it.fits(0, 0));
        }
        let croissant = c.items.iter().find(|it| it.name == "クロワッサン").expect("croissant");
        assert!(croissant.group.shapes.iter().filter(|s| s.kind == crate::model::Kind::Curve).count() >= 5);
        let spiral = c.items.iter().find(|it| it.name == "ソフトクリーム").expect("soft serve");
        assert!(spiral.group.shapes.iter().any(|s| s.kind == crate::model::Kind::Curve && s.path.len() >= 24));

        // Test both the OBJ/MTL and 3MF file serializers on the newly generated SDF geometry.
        // A coarse tolerance keeps CI work modest while exercising the actual mesh pipeline.
        for name in ["食パン", "クロワッサン", "ソフトクリーム"] {
            let it = c.items.iter().find(|it| it.name == name).expect("new food");
            for format in [Format::Obj, Format::ThreeMf] {
                let opts = Options {
                    format,
                    tolerance: 1.2,
                    lighten: Lighten::Flat,
                    scope: Scope::Visible,
                    ..Options::default()
                };
                let built = export_job::build(&it.scene(), None, &opts, &Progress::new())
                    .unwrap_or_else(|e| panic!("{name} {format:?} meshing failed: {e}"));
                assert!(built.closed && built.tris > 20, "{name} {format:?}: open or empty mesh");
                let path = Path::new(if format == Format::Obj { "food.obj" } else { "food.3mf" });
                let files = export_job::serialize(&built, &opts, path)
                    .unwrap_or_else(|e| panic!("{name} {format:?} export failed: {e}"));
                assert!(!files.is_empty() && files.iter().all(|(_, bytes)| !bytes.is_empty()));
                if format == Format::Obj {
                    let obj = std::str::from_utf8(&files[0].1).expect("OBJ is UTF-8");
                    assert!(obj.lines().any(|line| line.starts_with("v ")));
                    assert!(obj.lines().any(|line| line.starts_with("f ")));
                } else {
                    assert!(files[0].1.starts_with(b"PK"), "3MF must be ZIP");
                }
            }
        }
    }

    #[test]
    fn placing_moves_the_middle_and_keeps_the_height() {
        let c = parse(FILES[0]).unwrap_or(Category { name: String::new(), items: Vec::new() });
        for it in c.items.iter().take(3) {
            let g = it.placed([100.0, -50.0]);
            let n = g.shapes.len() as f32;
            let mx = g.shapes.iter().map(|s| s.pos[0]).sum::<f32>() / n;
            let my = g.shapes.iter().map(|s| s.pos[1]).sum::<f32>() / n;
            assert!((mx - 100.0).abs() < 1e-3 && (my + 50.0).abs() < 1e-3);
            assert_eq!(g.shapes[0].pos[2], it.group.shapes[0].pos[2]);
        }
    }

    #[test]
    fn search_matches_names_and_tags() {
        let it = Item { name: "いぬ".into(), tags: vec!["Dog".into()], group: Group::new("x") };
        assert!(it.matches("") && it.matches("いぬ") && it.matches("dog") && !it.matches("ねこ"));
    }
}
