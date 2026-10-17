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


    /// Smoke-test the ten common animals through the same paths as search, placement,
    /// scene save/load and thumbnail creation. Geometry export is checked for the two new models.
    #[test]
    fn familiar_animals_are_searchable_placeable_and_round_trip() {
        let category = parse(FILES[2]).expect("animal category");
        assert_eq!(category.name, "どうぶつ");
        for (name, query) in [
            ("いぬ", "dog"), ("ねこ", "cat"), ("うさぎ", "rabbit"), ("くま", "bear"),
            ("パンダ", "panda"), ("ぞう", "elephant"), ("きりん", "giraffe"),
            ("ペンギン", "penguin"), ("うま", "horse"), ("さかな", "fish"),
        ] {
            let it = category.items.iter().find(|it| it.name == name).expect("missing animal model");
            assert!(it.matches(query), "{name} search failed");
            assert!(it.fits(0, 0), "{name} exceeds scene limits");
            let scene = it.scene();
            let encoded = serde_json::to_string(&scene).expect("save scene");
            let decoded: Scene = serde_json::from_str(&encoded).expect("reopen scene");
            assert_eq!(decoded, scene, "{name} changed after reload");
            let placed = it.placed([80.0, -50.0]);
            assert_eq!(placed.shapes.len(), it.group.shapes.len());
            assert_eq!(placed.shapes[0].pos[2], it.group.shapes[0].pos[2]);
            assert_eq!(crate::cpu_render::thumb(&scene, 24).len(), 24 * 24 * 4);
        }
    }

    #[test]
    fn new_animals_can_export_obj_and_3mf() {
        use crate::export_formats::Format;
        use crate::export_job::{self, Lighten, Options};
        use crate::export_mesh::Progress;
        use std::path::Path;

        let category = parse(FILES[2]).expect("animal category");
        for name in ["パンダ", "うま"] {
            let it = category.items.iter().find(|it| it.name == name).expect("missing new model");
            for (format, ext, magic) in [
                (Format::Obj, "obj", &b"# MELTCAD"[..]),
                (Format::ThreeMf, "3mf", &b"PK"[..]),
            ] {
                let opts = Options {
                    format,
                    tolerance: 1.5,
                    lighten: Lighten::Off,
                    ..Options::default()
                };
                let built = export_job::build(&it.scene(), None, &opts, &Progress::new())
                    .unwrap_or_else(|e| panic!("{name}: {format:?} mesh failed: {e}"));
                assert!(built.tris > 0, "{name} has no mesh faces");
                let path = format!("animal.{ext}");
                let files = export_job::serialize(&built, &opts, Path::new(&path))
                    .unwrap_or_else(|e| panic!("{name}: {format:?} writing failed: {e}"));
                assert!(files.iter().any(|(p, bytes)| p.extension().is_some_and(|e| e == ext)
                    && bytes.starts_with(magic)), "{name}: invalid {ext} output");
            }
        }
    }

    #[test]
    fn search_matches_names_and_tags() {
        let it = Item { name: "いぬ".into(), tags: vec!["Dog".into()], group: Group::new("x") };
        assert!(it.matches("") && it.matches("いぬ") && it.matches("dog") && !it.matches("ねこ"));
    }
}
