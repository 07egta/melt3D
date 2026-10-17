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
    #[allow(dead_code, reason = "Preserve parsed category metadata for bulk callers; the UI uses the generated catalog.")]
    pub name: String,
    pub items: Vec<Item>,
}

/// Names and normalized search terms generated from the same JSON as the models.
/// The catalog contains no Group/Shape allocations and is immediately available.
pub struct ItemIndex {
    pub name: &'static str,
    pub tags: &'static [&'static str],
    lower_name: &'static str,
    lower_tags: &'static [&'static str],
}

impl ItemIndex {
    /// Same name-or-individual-tag substring semantics as Item::matches.
    pub fn matches(&self, normalized_query: &str) -> bool {
        normalized_query.is_empty() || self.lower_name.contains(normalized_query) || self.lower_tags.iter().any(|tag| tag.contains(normalized_query))
    }
}

pub struct CategoryIndex {
    pub name: &'static str,
    pub items: &'static [ItemIndex],
}

// Both arrays are generated together, preserving exact category and model order.
include!(concat!(env!("OUT_DIR"), "/library_catalog.rs"));

#[cfg(test)]
#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

static CATEGORIES: [OnceLock<Result<Category, String>>; FILES.len()] = [const { OnceLock::new() }; FILES.len()];

pub fn catalog() -> &'static [CategoryIndex] {
    &CATALOG
}

/// Parse and sanitize only the requested category. A click always has its model
/// before insertion: no worker failure or loading race can consume the action.
pub fn category(index: usize) -> Option<&'static Category> {
    let slot = CATEGORIES.get(index)?;
    let source = FILES.get(index)?;
    slot.get_or_init(|| parse(source)).as_ref().ok()
}

pub fn item(category_index: usize, item_index: usize) -> Option<&'static Item> {
    category(category_index)?.items.get(item_index)
}

/// Legacy bulk callers share exactly the same category allocations as the UI.
/// A damaged file is skipped, never a crash; tests check all bundled files parse.
pub fn all() -> &'static [&'static Category] {
    static LIB: OnceLock<Vec<&'static Category>> = OnceLock::new();
    LIB.get_or_init(|| (0..FILES.len()).filter_map(category).filter(|c| !c.items.is_empty()).collect())
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
    #[allow(dead_code)] // Compatibility and independent metadata-index parity checks.
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

    /// Run in a fresh process with only this ignored test selected. Times cover
    /// metadata/model preparation, not a rendered frame or operating-system I/O.
    #[test]
    #[ignore]
    fn benchmark_library_initial_access() {
        let start = std::time::Instant::now();
        let metadata = std::hint::black_box(catalog());
        let metadata_ns = start.elapsed().as_nanos();
        let start = std::time::Instant::now();
        let first = std::hint::black_box(category(0).expect("first category"));
        let first_category_us = start.elapsed().as_secs_f64() * 1e6;
        let start = std::time::Instant::now();
        let bulk = std::hint::black_box(all());
        let remaining_categories_us = start.elapsed().as_secs_f64() * 1e6;
        println!(
            "LIBRARY_BENCH {}",
            serde_json::json!({
                "metadata_ns": metadata_ns,
                "first_category_us": first_category_us,
                "remaining_categories_us": remaining_categories_us,
                "category_count": metadata.len(),
                "first_category_items": first.items.len(),
                "item_count": bulk.iter().map(|c| c.items.len()).sum::<usize>()
            })
        );
    }

    #[test]
    fn compact_embedded_library_preserves_every_json_value() {
        let names = [
            "people",
            "hands",
            "animals",
            "tools_a",
            "tools_b",
            "furniture",
            "food",
            "vehicles",
            "buildings",
            "nature",
            "print",
            "pip",
            "realistic",
            "toon",
            "town",
        ];
        assert_eq!(names.len(), FILES.len());
        for (name, embedded) in names.iter().zip(FILES) {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("library").join(format!("{name}.json"));
            let source = std::fs::read_to_string(path).expect("source JSON");
            let before: serde_json::Value = serde_json::from_str(&source).expect("source valid");
            let after: serde_json::Value = serde_json::from_str(embedded).expect("embedded valid");
            assert_eq!(after, before, "all geometry/material data preserved: {name}");
            assert!(embedded.len() <= source.len());
        }
    }

    #[test]
    fn metadata_catalog_preserves_labels_order_tags_and_search() {
        assert_eq!(catalog().len(), FILES.len());
        for (index, metadata) in catalog().iter().enumerate() {
            let models = category(index).expect("valid category");
            assert_eq!(metadata.name, models.name);
            assert_eq!(metadata.items.len(), models.items.len());
            for (meta, model) in metadata.items.iter().zip(&models.items) {
                assert_eq!(meta.name, model.name);
                assert!(meta.tags.iter().copied().eq(model.tags.iter().map(String::as_str)));
                let mut queries = vec![String::new(), " e ".into(), "  犬  ".into(), model.name.to_uppercase()];
                queries.extend(model.tags.iter().map(|tag| format!("  {}  ", tag.to_uppercase())));
                for query in queries {
                    assert_eq!(meta.matches(&query.trim().to_lowercase()), model.matches(&query), "{} / {query}", meta.name);
                }
            }
        }
        let meta = ItemIndex { name: "red", tags: &["blue"], lower_name: "red", lower_tags: &["blue"] };
        assert!(!meta.matches("red blue"), "search must not join separate name/tag fields");
    }

    #[test]
    fn bulk_and_single_item_access_share_category_allocations() {
        let bulk = all();
        assert_eq!(bulk.len(), FILES.len());
        for (index, from_bulk) in bulk.iter().enumerate() {
            let single = category(index).expect("valid category");
            assert!(std::ptr::eq(*from_bulk, single), "no duplicate parsed category cache");
            for (item_index, model) in single.items.iter().enumerate() {
                assert!(std::ptr::eq(item(index, item_index).expect("model"), model));
            }
        }
        assert!(category(usize::MAX).is_none());
        assert!(item(0, usize::MAX).is_none());
    }

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

    #[test]
    fn search_matches_names_and_tags() {
        let it = Item { name: "いぬ".into(), tags: vec!["Dog".into()], group: Group::new("x") };
        assert!(it.matches("") && it.matches("いぬ") && it.matches("dog") && !it.matches("ねこ"));
    }
}
