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

pub const ERR_TOO_MANY_SHAPES: &str = "読み込んだメッシュは1つのシーンに8個までです";
pub const ERR_TOO_BIG: &str = "メッシュのデータが多すぎます。ほかのメッシュを消すか、細かさをさげてください";

/// A baked distance volume shared between shapes. Equal when it is the very same volume.
#[derive(Clone)]
pub struct SharedVolume(Arc<Volume>);

impl SharedVolume {
    pub fn new(v: Volume) -> SharedVolume {
        SharedVolume(Arc::new(v))
    }
    pub fn from_arc(v: Arc<Volume>) -> SharedVolume {
        SharedVolume(v)
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
fn registry() -> &'static Mutex<HashMap<(u64, usize), Weak<Volume>>> {
    static REG: OnceLock<Mutex<HashMap<(u64, usize), Weak<Volume>>>> = OnceLock::new();
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
    if let Ok(mut r) = registry().lock() {
        if r.len() > 64 {
            r.retain(|_, w| w.strong_count() > 0);
        }
        r.insert((fnv1a(text.as_bytes()), text.len()), Arc::downgrade(v.arc()));
    }
}

/// A volume from its file text: shared when it is alive already, else decoded (within the limits).
fn volume_from_text(text: &str) -> Option<SharedVolume> {
    if text.len() > MAX_SERIAL_CHARS || text.is_empty() {
        return None;
    }
    let key = (fnv1a(text.as_bytes()), text.len());
    let alive = registry().lock().ok().and_then(|r| r.get(&key).and_then(Weak::upgrade));
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

#[cfg(test)]
mod tests {
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
        assert!(MAX_TOTAL_NODES < (1 << 24), "a node number is exact in an f32");
        assert!(MAX_TOTAL_NODES <= crate::gpu::VOL_W * crate::gpu::VOL_H * 4);
    }
}
