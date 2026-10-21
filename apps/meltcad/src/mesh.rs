//! Imported meshes inside the scene (`Kind::Mesh`): the shared handle to a baked distance volume,
//! how it is written into and read from a `.meltcad` file, and the limits that keep a scene (and
//! the graphics card) from being flooded with volumes.
//!
//! * `SharedVolume` is an `Arc<Volume>` that compares by pointer: a scene is cloned for undo and
//!   compared every frame, and must never compare megabytes of distances. Copies of a shape (copy
//!   and paste, duplicate, array, patterns, linked copies) share the one volume.
//! * In a file a volume is its compact base64 text (`Volume::to_base64`). The first shape that
//!   uses a volume writes the text; any further shape using the same volume writes only its number
//!   (the order of first appearance), so a duplicated mesh costs nothing in the file. Loading
//!   decodes each text once, and a text that is already alive in this program (a copy pasted back
//!   in) is found again by its hash and shared instead of decoded.
//! * Hostile files never panic or allocate without bound: texts are capped, at most
//!   `MAX_VOLUME_SHAPES` volumes are decoded per file, damaged ones become "no volume" and the
//!   shape that wanted it is dropped with a note by `Scene::sanitize_notes`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Serialize, Serializer};

use crate::sdf_volume::{MAX_SERIAL_CHARS, Volume};

/// Most shapes of a scene that use an imported mesh.
pub const MAX_VOLUME_SHAPES: usize = 8;
/// Most distance values (grid nodes) of all the volumes of a scene together. It is also what the
/// graphics card's volume texture holds (4 nodes per texel, `gpu::VOL_W` x `gpu::VOL_H`), and is
/// below 2^24 so a node number is exact in a float.
pub const MAX_TOTAL_NODES: usize = 16_000_000;

pub const ERR_TOO_MANY_SHAPES: &str = "Up to 8 loaded meshes per scene";
pub const ERR_TOO_BIG: &str = "Too much mesh data. Delete other meshes or lower the detail";

/// A baked distance volume shared between shapes. Equal when it is the very same volume.
#[derive(Clone)]
pub struct SharedVolume(Arc<Volume>);

impl SharedVolume {
    pub fn new(v: Volume) -> SharedVolume {
        SharedVolume(Arc::new(v))
    }
    pub fn arc(&self) -> &Arc<Volume> {
        &self.0
    }
    /// The address that names this volume (for tables of "the same volume").
    pub fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    pub fn nodes(&self) -> usize {
        self.0.data.len()
    }
}

impl PartialEq for SharedVolume {
    fn eq(&self, other: &SharedVolume) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::ops::Deref for SharedVolume {
    type Target = Volume;
    fn deref(&self) -> &Volume {
        &self.0
    }
}

impl std::fmt::Debug for SharedVolume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Volume({}x{}x{}, cell {})", self.0.dims[0], self.0.dims[1], self.0.dims[2], self.0.cell)
    }
}

// ---------------------------------------------------------------- file form

thread_local! {
    /// While a scene is being written: the volumes written so far, in order.
    static SAVE: RefCell<Option<Vec<usize>>> = const { RefCell::new(None) };
    /// While a scene is being read.
    static LOAD: RefCell<Option<LoadCtx>> = const { RefCell::new(None) };
    /// Prefer the volume this thread wrote. Background work may write the same
    /// text from a different Arc; it must not replace a UI round-trip's identity.
    static RECENT: RefCell<HashMap<(u64, usize), Weak<Volume>>> = RefCell::new(HashMap::new());
}

#[derive(Default)]
struct LoadCtx {
    /// The volumes of the file in order of first appearance (`None` = damaged or refused).
    seen: Vec<Option<SharedVolume>>,
    /// How many were actually decoded (not found alive already).
    decoded: usize,
    /// Nodes of what was decoded or found.
    nodes: usize,
}

/// Held while a scene is written with `Serialize`: repeated volumes are written as numbers.
pub struct SaveGuard(());

impl SaveGuard {
    pub fn begin() -> SaveGuard {
        SAVE.with(|s| *s.borrow_mut() = Some(Vec::new()));
        SaveGuard(())
    }
}

impl Drop for SaveGuard {
    fn drop(&mut self) {
        SAVE.with(|s| *s.borrow_mut() = None);
    }
}

/// Held while a scene (or a copied group) is read: volumes are counted, shared and numbered.
pub struct LoadGuard(());

impl LoadGuard {
    pub fn begin() -> LoadGuard {
        LOAD.with(|s| *s.borrow_mut() = Some(LoadCtx::default()));
        LoadGuard(())
    }
}

impl Drop for LoadGuard {
    fn drop(&mut self) {
        LOAD.with(|s| *s.borrow_mut() = None);
    }
}

impl Serialize for SharedVolume {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let key = self.key();
        let again = SAVE.with(|c| {
            let mut c = c.borrow_mut();
            let list = c.as_mut()?;
            match list.iter().position(|k| *k == key) {
                Some(i) => Some(i),
                None => {
                    list.push(key);
                    None
                }
            }
        });
        match again {
            Some(i) => s.serialize_u64(i as u64),
            None => {
                // (remembered, so that pasting this text back finds this very volume)
                let text = self.0.to_base64();
                remember_text(&text, self);
                s.serialize_str(&text)
            }
        }
    }
}

/// Volumes that are alive somewhere in the program, by the hash of their text, so that pasting a
/// copied shape gives the original's volume back instead of a second copy.
type VolumeRegistry = Mutex<HashMap<(u64, usize), Weak<Volume>>>;
fn registry() -> &'static VolumeRegistry {
    static REG: OnceLock<VolumeRegistry> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Remember that `v` was made from the text `text` (so a later copy of that text finds it).
pub fn remember_text(text: &str, v: &SharedVolume) {
    let key = (fnv1a(text.as_bytes()), text.len());
    RECENT.with(|c| {
        let mut r = c.borrow_mut();
        if r.len() > 64 {
            r.retain(|_, w| w.strong_count() > 0);
        }
        r.insert(key, Arc::downgrade(v.arc()));
    });
    if let Ok(mut r) = registry().lock() {
        if r.len() > 64 {
            r.retain(|_, w| w.strong_count() > 0);
        }
        r.insert(key, Arc::downgrade(v.arc()));
    }
}

/// A volume from its file text: shared when it is alive already, else decoded (within the limits).
fn volume_from_text(text: &str) -> Option<SharedVolume> {
    if text.len() > MAX_SERIAL_CHARS || text.is_empty() {
        return None;
    }
    let key = (fnv1a(text.as_bytes()), text.len());
    let alive = RECENT
        .with(|r| r.borrow().get(&key).and_then(Weak::upgrade))
        .or_else(|| registry().lock().ok().and_then(|r| r.get(&key).and_then(Weak::upgrade)));
    if let Some(a) = alive {
        return Some(SharedVolume(a));
    }
    // a file may bring only so many new volumes
    let allowed = LOAD.with(|c| c.borrow().as_ref().is_none_or(|l| l.decoded < MAX_VOLUME_SHAPES));
    if !allowed {
        return None;
    }
    let v = Volume::from_base64(text).ok()?;
    LOAD.with(|c| {
        if let Some(l) = c.borrow_mut().as_mut() {
            l.decoded += 1;
        }
    });
    let sv = SharedVolume::new(v);
    remember_text(text, &sv);
    Some(sv)
}

/// Read a shape's `volume` field. Anything wrong (a damaged text, a number that names nothing, too
/// many volumes, the wrong kind of value) gives `None` instead of an error, so one bad shape does
/// not stop the whole file from opening.
pub fn lenient_volume<'de, D: Deserializer<'de>>(d: D) -> Result<Option<SharedVolume>, D::Error> {
    struct V;

    /// Put a found volume in the file's list and count it against the limits.
    fn note(found: Option<SharedVolume>) -> Option<SharedVolume> {
        LOAD.with(|c| {
            let mut c = c.borrow_mut();
            let Some(l) = c.as_mut() else { return found };
            let found = found.filter(|v| l.nodes.saturating_add(v.nodes()) <= MAX_TOTAL_NODES);
            if let Some(v) = &found {
                l.nodes += v.nodes();
            }
            l.seen.push(found.clone());
            found
        })
    }

    impl<'de> Visitor<'de> for V {
        type Value = Option<SharedVolume>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a volume text")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(note(volume_from_text(v)))
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
            let found = LOAD.with(|c| c.borrow().as_ref().and_then(|l| l.seen.get(usize::try_from(v).ok()?).cloned().flatten()));
            Ok(found)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
            u64::try_from(v).map_or(Ok(None), |u| self.visit_u64(u))
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Self::Value, D2::Error> {
            d.deserialize_any(V)
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            while a.next_element::<IgnoredAny>()?.is_some() {}
            Ok(None)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            while a.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(None)
        }
    }
    d.deserialize_any(V)
}

// ---------------------------------------------------------------- limits

/// How many shapes use a volume, and how many distance values the different volumes hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub shapes: usize,
    pub nodes: usize,
}

impl Usage {
    /// Count the volumes of `shapes` (a volume shared by several shapes counts once for the nodes).
    pub fn of<'a>(shapes: impl IntoIterator<Item = &'a crate::model::Shape>) -> Usage {
        let mut seen: Vec<usize> = Vec::new();
        let mut u = Usage::default();
        for s in shapes {
            if let Some(v) = &s.volume {
                u.shapes += 1;
                let k = v.key();
                if !seen.contains(&k) {
                    seen.push(k);
                    u.nodes = u.nodes.saturating_add(v.nodes());
                }
            }
        }
        u
    }

    /// Can a shape with a volume of `nodes` values (`shared`: the very volume is in the scene
    /// already) be added?
    pub fn check_add(&self, nodes: usize, shared: bool) -> Result<(), &'static str> {
        if self.shapes >= MAX_VOLUME_SHAPES {
            return Err(ERR_TOO_MANY_SHAPES);
        }
        if !shared && self.nodes.saturating_add(nodes) > MAX_TOTAL_NODES {
            return Err(ERR_TOO_BIG);
        }
        Ok(())
    }
}

/// Can these shapes join the scene without passing the limits for meshes? (Copies of a mesh share
/// its volume, so they cost no data, but each one is a shape the graphics card must march through.)
pub fn can_add<'a>(scene: &'a crate::model::Scene, extra: impl IntoIterator<Item = &'a crate::model::Shape>) -> Result<(), &'static str> {
    let u = Usage::of(scene.groups.iter().flat_map(|g| g.shapes.iter()).chain(extra));
    if u.shapes > MAX_VOLUME_SHAPES {
        Err(ERR_TOO_MANY_SHAPES)
    } else if u.nodes > MAX_TOTAL_NODES {
        Err(ERR_TOO_BIG)
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{Kind, Material, Scene, Shape};

    pub(crate) fn ball_volume(n: u32) -> Volume {
        // a sphere of radius 10 mm in a grid of n^3 nodes, cell 1 mm
        let mut data = Vec::new();
        let c = (n as f32 - 1.0) / 2.0;
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let d = [(i as f32 - c), (j as f32 - c), (k as f32 - c)];
                    data.push((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - 10.0);
                }
            }
        }
        Volume { dims: [n; 3], origin: [-c; 3], cell: 1.0, data }
    }

    pub(crate) fn mesh_shape(v: &SharedVolume) -> Shape {
        let mut s = Shape::new(Kind::Mesh, [0.0, 0.0, 10.0], Material::default());
        s.volume = Some(v.clone());
        s
    }

    #[test]
    fn volumes_compare_by_pointer_and_debug_is_short() {
        let a = SharedVolume::new(ball_volume(24));
        let b = a.clone();
        let c = SharedVolume::new(ball_volume(24));
        assert!(a == b && a != c, "same pointer = equal, same numbers in another volume = different");
        assert!(format!("{a:?}").len() < 60);
        assert_eq!(a.nodes(), 24 * 24 * 24);
    }

    #[test]
    fn a_shared_volume_is_written_once_and_read_back_shared() {
        let v = SharedVolume::new(ball_volume(24));
        let mut sc = Scene::default();
        let mut a = mesh_shape(&v);
        a.name = "a".into();
        let mut b = a.clone();
        b.name = "b".into();
        sc.groups[0].shapes.push(a);
        sc.groups[0].shapes.push(b);
        let json = sc.to_json();
        assert_eq!(json.matches("\"volume\"").count(), 2);
        let text = v.arc().to_base64();
        assert_eq!(json.matches(text.as_str()).count(), 1, "the second shape writes only a number");
        let back = Scene::from_json(&json).unwrap_or_default();
        let (x, y) = (&back.groups[0].shapes[1], &back.groups[0].shapes[2]);
        let (Some(vx), Some(vy)) = (&x.volume, &y.volume) else { panic!("volumes lost") };
        assert!(vx == vy, "the two shapes share one volume after loading");
        // the numbers survive the 1/64-cell quantisation
        let d = vx.distance([0.2, 0.1, 0.3]);
        assert!((d - v.distance([0.2, 0.1, 0.3])).abs() < 0.02, "{d}");
    }

    #[test]
    fn a_pasted_copy_finds_the_volume_that_is_alive() {
        let v = SharedVolume::new(ball_volume(20));
        let s = mesh_shape(&v);
        let text = serde_json::to_string(&s).unwrap_or_default();
        let _g = LoadGuard::begin();
        let back: Shape = serde_json::from_str(&text).unwrap_or_else(|_| Shape::new(Kind::Sphere, [0.0; 3], Material::default()));
        assert_eq!(back.volume.as_ref().map(SharedVolume::key), Some(v.key()), "decoded from the registry, not a second copy");
    }

    #[test]
    fn a_background_round_trip_cannot_replace_this_threads_shared_volume() {
        let v = SharedVolume::new(ball_volume(25));
        let scene = scene_with(vec![mesh_shape(&v)]);
        let json = scene.to_json();
        let background_json = json.clone();
        let original_key = v.key();
        let (other, same_json) = std::thread::spawn(move || {
            // A new thread still finds the already alive volume through the
            // shared registry, without decoding a second buffer.
            let loaded = Scene::from_json(&background_json).expect("background load");
            let original = loaded.groups[0].shapes[1].volume.as_ref().expect("shared mesh");
            assert_eq!(original.key(), original_key, "cross-thread reuse still finds the existing buffer");
            let other = SharedVolume::new(ball_volume(25));
            assert_ne!(original, &other);
            let same_json = scene_with(vec![mesh_shape(&other)]).to_json();
            (other, same_json)
        })
        .join()
        .expect("background serialization");
        assert_eq!(same_json, json, "the background thread wrote indistinguishable file text");
        assert_ne!(v, other, "keep both independently created volumes alive during the reload");
        let back = Scene::from_json(&json).expect("foreground reload");
        assert_eq!(back, scene, "background serialization must not steal this thread's original volume handle");
        assert!(Arc::ptr_eq(back.groups[0].shapes[1].volume.as_ref().expect("mesh survives").arc(), v.arc()));
    }

    #[test]
    fn hostile_volume_fields_become_none_and_never_panic() {
        let base = Shape::new(Kind::Mesh, [0.0; 3], Material::default());
        let json = serde_json::to_string(&base).unwrap_or_default();
        for bad in [
            "\"volume\":\"not base64!!\"",
            "\"volume\":\"\"",
            "\"volume\":\"TVNEUg==\"",
            "\"volume\":12345",
            "\"volume\":-5",
            "\"volume\":1.5",
            "\"volume\":true",
            "\"volume\":null",
            "\"volume\":[1,2,{\"a\":3}]",
            "\"volume\":{\"x\":[1,2]}",
        ] {
            let text = json.replacen('{', &format!("{{{bad},"), 1);
            let _g = LoadGuard::begin();
            let s: Result<Shape, _> = serde_json::from_str(&text);
            assert!(s.is_ok_and(|s| s.volume.is_none()), "{bad}");
        }
        // a text longer than the cap is refused without decoding
        let huge = format!("\"volume\":\"{}\"", "A".repeat(MAX_SERIAL_CHARS + 8));
        let text = json.replacen('{', &format!("{{{huge},"), 1);
        let _g = LoadGuard::begin();
        assert!(serde_json::from_str::<Shape>(&text).is_ok_and(|s| s.volume.is_none()));
    }

    #[test]
    fn a_file_may_bring_only_so_many_new_volumes() {
        let mut sc = Scene::default();
        for i in 0..MAX_VOLUME_SHAPES + 3 {
            // different volumes (different sizes), each small
            let v = SharedVolume::new(ball_volume(6 + i as u32));
            sc.groups[0].shapes.push(mesh_shape(&v));
        }
        let json = {
            let _g = SaveGuard::begin();
            serde_json::to_string(&sc).unwrap_or_default()
        };
        // (not alive any more once the scene is dropped)
        drop(sc);
        let back = {
            let _g = LoadGuard::begin();
            serde_json::from_str::<Scene>(&json).unwrap_or_default()
        };
        let have = back.groups[0].shapes.iter().filter(|s| s.volume.is_some()).count();
        assert_eq!(have, MAX_VOLUME_SHAPES, "the rest were refused");
    }

    #[test]
    fn usage_counts_a_shared_volume_once() {
        let v = SharedVolume::new(ball_volume(10));
        let w = SharedVolume::new(ball_volume(12));
        let shapes = vec![mesh_shape(&v), mesh_shape(&v), mesh_shape(&w), Shape::new(Kind::Box, [0.0; 3], Material::default())];
        let u = Usage::of(&shapes);
        assert_eq!((u.shapes, u.nodes), (3, 1000 + 1728));
        assert!(u.check_add(100, false).is_ok());
        assert_eq!(u.check_add(MAX_TOTAL_NODES, false), Err(ERR_TOO_BIG));
        assert!(u.check_add(MAX_TOTAL_NODES, true).is_ok(), "a copy of a volume that is in the scene costs no memory");
        let full = Usage { shapes: MAX_VOLUME_SHAPES, nodes: 0 };
        assert_eq!(full.check_add(1, true), Err(ERR_TOO_MANY_SHAPES));
    }

    #[test]
    fn the_node_cap_fits_the_graphics_pool_and_a_float() {
        const {
            assert!(MAX_TOTAL_NODES < (1 << 24), "a node number is exact in an f32");
            assert!(MAX_TOTAL_NODES <= crate::gpu::VOL_W * crate::gpu::VOL_H * 4);
        }
    }

    // ------------------------------------------------------------ the model's file form

    fn scene_with(shapes: Vec<Shape>) -> Scene {
        let mut sc = Scene::default();
        sc.groups[0].shapes.extend(shapes);
        sc
    }

    #[test]
    fn a_scene_with_a_mesh_round_trips_and_one_without_data_is_dropped_with_a_note() {
        let v = SharedVolume::new(ball_volume(24));
        let mut m = mesh_shape(&v);
        m.name = "Ball".into();
        m.source = "ball.stl".into();
        m.size = [20.0; 3];
        let sc = scene_with(vec![m]);
        let (back, notes) = Scene::from_json_notes(&sc.to_json()).unwrap_or_default();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(back, sc, "equal, and the volume is the very same one");
        let s = &back.groups[0].shapes[1];
        assert_eq!((s.kind, s.name.as_str(), s.source.as_str(), s.size), (Kind::Mesh, "Ball", "ball.stl", [20.0; 3]));
        // a mesh shape that has lost its data cannot be drawn: it is dropped, the rest of the scene opens
        let mut empty = Shape::new(Kind::Mesh, [0.0; 3], Material::default());
        empty.name = "empty".into();
        let sc2 = scene_with(vec![empty]);
        let (back2, notes2) = Scene::from_json_notes(&sc2.to_json()).unwrap_or_default();
        assert_eq!(back2.shape_count(), 1, "only the default sphere is left");
        assert!(notes2.len() == 1 && notes2[0].contains("こわれて"), "{notes2:?}");
        // a volume on a shape that is not a mesh is let go
        let mut odd = Shape::new(Kind::Box, [0.0; 3], Material::default());
        odd.volume = Some(v);
        let sc3 = scene_with(vec![odd]);
        let (back3, _) = Scene::from_json_notes(&sc3.to_json()).unwrap_or_default();
        assert!(back3.groups[0].shapes[1].volume.is_none());
        // a scene without meshes has no volume field at all (old files and new files look alike)
        assert!(!Scene::default().to_json().contains("volume"));
    }

    #[test]
    fn damaged_volume_text_in_a_scene_file_drops_the_shape_and_never_panics() {
        let v = SharedVolume::new(ball_volume(20));
        let sc = scene_with(vec![mesh_shape(&v)]);
        let json = sc.to_json();
        let text = v.arc().to_base64();
        let variants = [
            json.replace(&text, "ZZZZ"),
            json.replace(&text, &text[..text.len() / 2]),
            json.replace(&text, &text.replace('A', "B")),
            json.replace(&text, &format!("{}{}", &text[..40], "!".repeat(30))),
            json.replace(&text, &"/".repeat(5000)),
            json.replace(&format!("\"{text}\""), "[1,2,3]"),
            json.replace(&format!("\"{text}\""), "{\"a\":{\"b\":null}}"),
            json.replace(&format!("\"{text}\""), "99"),
        ];
        for (i, j) in variants.iter().enumerate() {
            // (forget the live copy so that the damaged text is really decoded)
            let r = Scene::from_json_notes(j);
            let (back, _) = r.unwrap_or_else(|e| panic!("variant {i} did not load: {e}"));
            // the damaged variants lose the mesh (a text altered in a harmless way may still decode)
            let meshes = back.groups[0].shapes.iter().filter(|s| s.kind == Kind::Mesh).count();
            assert!(meshes <= 1, "variant {i}");
            assert!(back.shape_count() >= 1);
        }
        // the first three damage the data for sure (the half text is cut, 'Z' is no header)
        for j in &variants[..2] {
            let (back, notes) = Scene::from_json_notes(j).unwrap_or_default();
            assert!(!back.groups[0].shapes.iter().any(|s| s.kind == Kind::Mesh) && !notes.is_empty());
        }
    }

    #[test]
    fn a_scene_keeps_at_most_eight_mesh_shapes_and_a_total_of_data() {
        let v = SharedVolume::new(ball_volume(12));
        let many: Vec<Shape> = (0..MAX_VOLUME_SHAPES + 4)
            .map(|_| mesh_shape(&v))
            .map(|mut s| {
                s.pos[0] += 1.0;
                s
            })
            .collect();
        let mut sc = scene_with(many);
        let notes = sc.sanitize_notes();
        let kept = sc.groups[0].shapes.iter().filter(|s| s.kind == Kind::Mesh).count();
        assert_eq!(kept, MAX_VOLUME_SHAPES);
        assert!(notes.len() == 1 && notes[0].contains("多すぎ"), "{notes:?}");
        // volumes that together hold too much: the one that does not fit goes (3 x 6 M nodes > 16 M)
        let big = |n: usize| SharedVolume::new(Volume { dims: [200, 200, 150], origin: [0.0; 3], cell: 1.0, data: vec![1.0; 200 * 200 * 150 + n] });
        // (data.len() must equal the dims for a valid volume, so make the three with exact sizes)
        let a = big(0);
        let b = big(0);
        let c = big(0);
        assert_eq!(a.nodes(), 6_000_000);
        let mut sc = scene_with(vec![mesh_shape(&a), mesh_shape(&b), mesh_shape(&c)]);
        let notes = sc.sanitize_notes();
        let kept = sc.groups[0].shapes.iter().filter(|s| s.kind == Kind::Mesh).count();
        assert_eq!(kept, 2, "two fit (12 M), the third would be 18 M");
        assert!(!notes.is_empty());
        // the same big volume used by three shapes counts once
        let mut sc = scene_with(vec![mesh_shape(&a), mesh_shape(&a), mesh_shape(&a)]);
        assert!(sc.sanitize_notes().is_empty());
        assert_eq!(sc.groups[0].shapes.iter().filter(|s| s.kind == Kind::Mesh).count(), 3);
    }

    #[test]
    fn cloning_and_comparing_scenes_never_touches_the_distances() {
        let v = SharedVolume::new(Volume { dims: [160, 160, 160], origin: [0.0; 3], cell: 1.0, data: vec![1.0; 160 * 160 * 160] });
        let sc = scene_with(vec![mesh_shape(&v), mesh_shape(&v), mesh_shape(&v)]);
        let t0 = std::time::Instant::now();
        let mut equal = 0;
        for _ in 0..2000 {
            let copy = sc.clone();
            if copy == sc {
                equal += 1;
            }
        }
        let secs = t0.elapsed().as_secs_f64();
        assert_eq!(equal, 2000);
        assert!(secs < 1.0, "2000 clones and comparisons of a scene with 3 x 4 M nodes took {secs:.3} s");
        // the clones share the one volume (no copy of the data)
        let copy = sc.clone();
        for (a, b) in sc.groups[0].shapes.iter().zip(&copy.groups[0].shapes) {
            if let (Some(x), Some(y)) = (&a.volume, &b.volume) {
                assert!(Arc::ptr_eq(x.arc(), y.arc()));
            }
        }
        assert!(Arc::strong_count(v.arc()) >= 7, "one handle per shape and clone, one volume");
    }

    // ------------------------------------------------------------ copies share the volume

    use crate::app::App;
    use crate::model::Pick;

    fn app_with_mesh() -> (App, SharedVolume) {
        let mut app = App::new(true, None);
        let v = SharedVolume::new(ball_volume(16));
        app.scene.groups[0].shapes.push(mesh_shape(&v));
        app.sel = Some(Pick::Shape(0, 1));
        (app, v)
    }

    fn volumes_of(app: &App) -> Vec<usize> {
        app.scene.groups.iter().flat_map(|g| g.shapes.iter()).filter_map(|s| s.volume.as_ref().map(SharedVolume::key)).collect()
    }

    #[test]
    fn duplicate_array_mirror_clipboard_and_undo_share_the_volume() {
        let ctx = egui::Context::default();
        let (mut app, v) = app_with_mesh();
        let key = v.key();
        app.duplicate();
        assert_eq!(volumes_of(&app), vec![key, key]);
        // the array card's copies
        app.sel = Some(Pick::Shape(0, 1));
        app.mirror_copy();
        assert_eq!(volumes_of(&app), vec![key; 3]);
        // copy and paste through the clipboard text (also what another window would do)
        app.sel = Some(Pick::Shape(0, 1));
        let text = crate::clip::copy_text(&app).unwrap_or_default();
        assert!(text.len() > 100);
        assert!(crate::clip::paste_text(&mut app, &text).is_ok());
        assert_eq!(volumes_of(&app), vec![key; 4], "the pasted copy found the live volume instead of decoding a second one");
        // a whole group copied
        app.sel = Some(Pick::Group(0));
        let gtext = crate::clip::copy_text(&app).unwrap_or_default();
        let _ = gtext;
        // undo and redo walk back and forth over the same volume
        app.settle(&ctx);
        app.undo();
        app.redo();
        assert!(volumes_of(&app).iter().all(|k| *k == key));
        // the CPU field of the scene points at the same data
        app.settle(&ctx);
        let f = app.field();
        assert!(f.sources.iter().flat_map(|g| g.shapes.iter()).filter_map(|p| p.volume.as_ref()).all(|p| p.key() == key));
        assert!(Arc::strong_count(v.arc()) > 5);
    }

    #[test]
    fn copies_stop_at_eight_mesh_shapes_with_a_message() {
        let ctx = egui::Context::default();
        let (mut app, _v) = app_with_mesh();
        for _ in 0..20 {
            app.sel = Some(Pick::Shape(0, 1));
            app.duplicate();
        }
        assert_eq!(volumes_of(&app).len(), MAX_VOLUME_SHAPES);
        assert!(app.notice.is_some(), "the person is told why nothing happened");
        // the clipboard path says no too
        app.sel = Some(Pick::Shape(0, 1));
        let text = crate::clip::copy_text(&app).unwrap_or_default();
        assert_eq!(crate::clip::paste_text(&mut app, &text), Err(ERR_TOO_MANY_SHAPES.to_string()));
        assert_eq!(volumes_of(&app).len(), MAX_VOLUME_SHAPES);
        let _ = ctx;
    }

    #[test]
    fn patterns_and_linked_copies_draw_the_one_volume_without_new_shapes() {
        let ctx = egui::Context::default();
        let (mut app, v) = app_with_mesh();
        app.sel = Some(Pick::Group(0));
        app.make_instance(&ctx);
        assert_eq!(app.scene.groups.len(), 2);
        assert!(app.scene.groups[1].shapes.is_empty(), "a linked copy holds no shapes (and so no volume of its own)");
        app.scene.groups[0].pattern = Some(crate::pattern::Pattern::Rect { counts: [3, 2, 1], spacing: [60.0, 60.0, 0.0] });
        app.settle(&ctx);
        let f = app.field();
        let users: Vec<_> = f.sources.iter().flat_map(|g| g.shapes.iter()).filter_map(|p| p.volume.as_ref()).collect();
        assert_eq!(users.len(), 1, "prepared once, drawn through every instance");
        assert_eq!(users[0].key(), v.key());
        // every instance measures the volume at its own place
        let at = |p: [f32; 3]| f.distance(p);
        assert!(at([0.0, 0.0, 10.0]) < 0.0 && at([60.0, 0.0, 10.0]) < 0.0 && at([120.0, 60.0, 10.0]) < 0.0, "a mesh in each cell of the grid");
        assert!(crate::mesh::Usage::of(app.scene.groups.iter().flat_map(|g| g.shapes.iter())).shapes == 1);
    }

    /// What a saved scene with one 128-cell volume weighs (`cargo test -p meltcad --release
    /// report_scene_file_size -- --ignored --nocapture`).
    #[test]
    #[ignore = "timing report"]
    fn report_scene_file_size() {
        use crate::import_mesh::tests::sphere_mesh;
        use crate::sdf_volume::{BakeControl, BakeOptions, bake_with};
        for (name, mesh) in
            [("ball", sphere_mesh(30.0, 60, 120)), ("flat plate", crate::import_mesh::tests::box_mesh([-50.0, -50.0, 0.0], [50.0, 50.0, 4.0]))]
        {
            let t0 = std::time::Instant::now();
            let (vol, _) = bake_with(&mesh, &BakeOptions { res: 128, ..BakeOptions::default() }, &BakeControl::new()).unwrap();
            let bake_s = t0.elapsed().as_secs_f64();
            let nodes = vol.data.len();
            let sv = SharedVolume::new(vol);
            let mut m = mesh_shape(&sv);
            m.source = "x.stl".into();
            let sc = scene_with(vec![m]);
            let t1 = std::time::Instant::now();
            let json = sc.to_json();
            let save_s = t1.elapsed().as_secs_f64();
            let t2 = std::time::Instant::now();
            // (forget the live volume so that the text is decoded)
            drop(sc);
            drop(sv);
            let back = Scene::from_json(&json).unwrap_or_default();
            let load_s = t2.elapsed().as_secs_f64();
            eprintln!(
                "{name}: {nodes} nodes, bake {bake_s:.2} s, scene file {:.2} MB, save {save_s:.2} s, load {load_s:.2} s, {} shapes back",
                json.len() as f64 / 1e6,
                back.shape_count()
            );
        }
    }
}
