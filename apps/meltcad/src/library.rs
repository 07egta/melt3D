//! The model library: ready-made things (people, hands, tools, furniture, food ...) that go into
//! the scene with one click. Each category is a JSON file in `apps/meltcad/library/`; see
//! `library/README.md` for the format. The files are compiled in, so nothing is read at run time.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use egui::Context;

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
    /// Bundled category name, kept for benchmark/test callers of all().
    #[allow(dead_code)]
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


/// Labels and file order are fixed by the bundled JSON data. They let the tabs be
/// drawn before any Group/Shape is deserialized.
const LABELS: [&str; 15] = [
    "ひと", "て", "どうぶつ", "どうぐ", "くらしのもの", "かぐ", "たべもの",
    "のりもの", "たてもの", "しぜん", "3Dプリント", "プリントインプレース",
    "リアルなひと", "デフォルメ", "まち",
];

pub fn count() -> usize {
    FILES.len()
}

pub fn label(index: usize) -> Option<&'static str> {
    LABELS.get(index).copied()
}

/// Each category is deserialized only after its first visible card requests it.
/// Errors are remembered so a broken built-in file cannot trigger infinite retries.
static CATEGORIES: [OnceLock<Result<Category, String>>; 15] =
    [const { OnceLock::new() }; 15];

pub fn loaded(index: usize) -> Option<Result<&'static Category, &'static str>> {
    CATEGORIES.get(index)?.get().map(|c| c.as_ref().map_err(String::as_str))
}

/// Search metadata deliberately does not contain full models.
pub struct SearchEntry {
    pub category: usize,
    pub item: usize,
    pub name: String,
    key: String,
}

impl SearchEntry {
    pub fn matches(&self, normalized_query: &str) -> bool {
        self.key.contains(normalized_query)
    }
}

pub struct SearchIndex {
    pub entries: Vec<SearchEntry>,
    pub failed_files: usize,
}

#[derive(Deserialize)]
struct IndexCategory {
    #[serde(default)]
    items: Vec<IndexItem>,
}

#[derive(Deserialize)]
struct IndexItem {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
}

/// serde ignores each item's \`group\`, so searching never builds CAD shapes.
fn index_file(text: &str, category: usize) -> Result<Vec<SearchEntry>, String> {
    let data: IndexCategory = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok(data.items.into_iter().enumerate().map(|(item, raw)| SearchEntry {
        category,
        item,
        key: format!("{} {}", raw.name.to_lowercase(), raw.tags.join(" ").to_lowercase()),
        name: raw.name,
    }).collect())
}

fn build_index() -> SearchIndex {
    let mut result = SearchIndex { entries: Vec::new(), failed_files: 0 };
    for (i, json) in FILES.iter().enumerate() {
        match index_file(json, i) {
            Ok(items) => result.entries.extend(items),
            Err(_) => result.failed_files += 1,
        }
    }
    result
}

static SEARCH: OnceLock<SearchIndex> = OnceLock::new();

pub fn search_index() -> Option<&'static SearchIndex> {
    SEARCH.get()
}

enum Request {
    Category(usize, Context),
    Search(Context),
}

struct Loader {
    tx: mpsc::Sender<Request>,
    pending: Arc<Mutex<HashSet<usize>>>,
    search_asked: AtomicBool,
}

/// Reuse one background loader for category JSON and the lightweight search index.
/// Its channel is created on the first visit to the library, not at process start.
fn loader() -> Option<&'static Loader> {
    static WORKER: OnceLock<Option<Loader>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let remaining = pending.clone();
        std::thread::Builder::new()
            .name("meltcad-library".into())
            .spawn(move || {
                for request in rx {
                    match request {
                        Request::Category(i, ctx) => {
                            if let Some(slot) = CATEGORIES.get(i) {
                                let parsed = std::panic::catch_unwind(|| parse(FILES[i]))
                                    .unwrap_or_else(|_| Err("モデルデータの解析に失敗しました".into()));
                                let _ = slot.set(parsed);
                            }
                            remaining.lock().unwrap_or_else(|e| e.into_inner()).remove(&i);
                            ctx.request_repaint();
                        }
                        Request::Search(ctx) => {
                            let index = std::panic::catch_unwind(build_index)
                                .unwrap_or(SearchIndex { entries: Vec::new(), failed_files: FILES.len() });
                            let _ = SEARCH.set(index);
                            ctx.request_repaint();
                        }
                    }
                }
            })
            .ok()?;
        Some(Loader { tx, pending, search_asked: AtomicBool::new(false) })
    }).as_ref()
}

/// Returns false if a worker could not be started. Such a failure leaves the
/// rest of the application usable and shows an explanatory placeholder.
pub fn request_category(index: usize, ctx: &Context) -> bool {
    if index >= FILES.len() {
        return false;
    }
    if loaded(index).is_some() {
        return true;
    }
    let Some(worker) = loader() else { return false };
    {
        let mut pending = worker.pending.lock().unwrap_or_else(|e| e.into_inner());
        if !pending.insert(index) {
            return true;
        }
    }
    if worker.tx.send(Request::Category(index, ctx.clone())).is_err() {
        worker.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&index);
        return false;
    }
    true
}

pub fn request_search(ctx: &Context) -> bool {
    if SEARCH.get().is_some() {
        return true;
    }
    let Some(worker) = loader() else { return false };
    if worker.search_asked.swap(true, Ordering::Relaxed) {
        return true;
    }
    if worker.tx.send(Request::Search(ctx.clone())).is_err() {
        worker.search_asked.store(false, Ordering::Relaxed);
        return false;
    }
    true
}

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
    #[allow(dead_code)] // Older callers can still perform an Item search directly.
    pub fn matches(&self, q: &str) -> bool {
        let q = q.trim().to_lowercase();
        q.is_empty() || self.name.to_lowercase().contains(&q) || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    /// The item as a scene of its own (for thumbnails and the test).
    pub fn scene(&self) -> Scene {
        Scene { groups: vec![self.group.clone()], ..Scene::default() }
    }

    /// Approximate XY midpoint of the visible additive geometry, not the average
    /// of shape origins (small eyes/fingers otherwise pull a model off centre).
    fn centre_xy(&self) -> [f32; 2] {
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for s in &self.group.shapes {
            if !s.solid() || s.op != crate::model::Op::Add {
                continue;
            }
            let r = crate::view::radius_of(s);
            for axis in 0..2 {
                lo[axis] = lo[axis].min(s.pos[axis] - r);
                hi[axis] = hi[axis].max(s.pos[axis] + r);
            }
        }
        if !lo[0].is_finite() || !lo[1].is_finite() {
            return [0.0, 0.0];
        }
        [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]
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
    fn labels_and_search_index_preserve_json_item_order() {
        for (i, data) in FILES.iter().enumerate() {
            let parsed = parse(data).expect("valid library category");
            assert_eq!(label(i), Some(parsed.name.as_str()));
            let metadata = index_file(data, i).expect("metadata only");
            assert_eq!(metadata.len(), parsed.items.len());
            for (j, (m, item)) in metadata.iter().zip(&parsed.items).enumerate() {
                assert_eq!(m.category, i);
                assert_eq!(m.item, j);
                assert_eq!(m.name, item.name);
                for tag in &item.tags {
                    assert!(m.matches(&tag.to_lowercase()));
                }
            }
        }
        assert_eq!(count(), FILES.len());
    }

    #[test]
    fn invalid_search_file_is_an_error_but_does_not_crash() {
        assert!(index_file("not-json", 0).is_err());
        assert_eq!(index_file(r#"{"items":[]}"#, 0).expect("empty").len(), 0);
    }

    #[test]
    fn placing_moves_the_geometry_midpoint_without_resizing_or_lifting_it() {
        let c = parse(FILES[0]).expect("valid library");
        for it in c.items.iter().take(3) {
            let g = it.placed([100.0, -50.0]);
            let placed = Item { name: it.name.clone(), tags: it.tags.clone(), group: g };
            let [mx, my] = placed.centre_xy();
            assert!((mx - 100.0).abs() < 1e-3 && (my + 50.0).abs() < 1e-3);
            for (before, after) in it.group.shapes.iter().zip(&placed.group.shapes) {
                assert_eq!(after.pos[2], before.pos[2]);
                assert_eq!(after.size, before.size);
                assert_eq!(after.scale, before.scale);
                assert_eq!(after.rot, before.rot);
            }
        }

        // Different-sized parts and many tiny details must not determine the
        // insertion position by the arithmetic mean of their centre points.
        let mut g = Group::new("片寄ったモデル");
        g.shapes.push(crate::model::Shape::new(crate::model::Kind::Sphere, [-20.0, 0.0, 20.0], crate::model::Material::default()));
        g.shapes.push(crate::model::Shape::new(crate::model::Kind::Box, [45.0, 0.0, 20.0], crate::model::Material::default()));
        let it = Item { name: "テスト".into(), tags: vec![], group: g };
        let placed = Item { name: it.name.clone(), tags: vec![], group: it.placed([0.0, 0.0]) };
        assert!(placed.centre_xy().iter().all(|x| x.abs() < 1e-3));
    }

    #[test]
    fn search_matches_names_and_tags() {
        let it = Item { name: "いぬ".into(), tags: vec!["Dog".into()], group: Group::new("x") };
        assert!(it.matches("") && it.matches("いぬ") && it.matches("dog") && !it.matches("ねこ"));
    }
    #[test]
    fn everyday_vehicle_library_loads_places_saves_and_renders_thumbnails() {
        let cat = all().iter().find(|c| c.name == "のりもの").expect("vehicle category");
        let names = ["セダン", "軽自動車", "SUV", "バス", "トラック", "自転車", "バイク", "電車", "飛行機", "船"];
        let mut distinct = std::collections::HashSet::new();
        for it in &cat.items {
            assert!(distinct.insert(it.name.as_str()), "duplicate model {}", it.name);
        }
        for name in names {
            let it = cat.items.iter().find(|it| it.name == name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(!it.group.shapes.is_empty() && it.group.shapes.len() <= 40);
            assert!(it.fits(0, 0));
            let placed = it.placed([25.0, -10.0]);
            assert_eq!(placed.shapes.len(), it.group.shapes.len());
            let json = it.scene().to_json();
            let loaded = Scene::from_json(&json).expect("save/load round trip");
            assert_eq!(loaded.groups.len(), 1);
            assert_eq!(loaded.groups[0].shapes.len(), it.group.shapes.len());
            let pixels = crate::cpu_render::thumb(&loaded, 16);
            assert_eq!(pixels.len(), 16 * 16 * 4);
        }
    }
}
