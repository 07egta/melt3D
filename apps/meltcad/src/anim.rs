//! Animation: poses on a timeline, in between frames made by the program.
//!
//! The way of clay animation: put the scene in a pose, store it at a frame, move on, pose it again.
//! Every shape (found by its `uid`) keeps its own list of keys, so a shape that joins later just
//! starts moving from its first key. The melt amount and drip are animated too, so the whole
//! piece can melt over time. "Clay" holds each pose for a few frames, like real stop motion.

use serde::{Deserialize, Serialize};

use crate::model::Scene;
use crate::pattern::Xform;

pub const MAX_FRAMES: u32 = 2400;
pub const MAX_KEYS: usize = 600;
pub const MAX_TRACKS: usize = 512;
/// With "clay" on, a pose is held for this many frames.
pub const CLAY_HOLD: u32 = 3;

/// What moves on one shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vals {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub size: [f32; 3],
    pub scale: [f32; 3],
    pub color: [f32; 3],
    pub round: f32,
}

/// Turn from `a` to `b` degrees the short way when they are less than a full turn apart
/// (350 -> 10 goes 20 degrees forward, not 340 back). Bigger gaps are kept as typed so a
/// deliberate spin still spins.
fn turn(a: f32, b: f32, t: f32) -> f32 {
    let mut d = b - a;
    if d > 180.0 && d < 360.0 {
        d -= 360.0;
    } else if d < -180.0 && d > -360.0 {
        d += 360.0;
    }
    a + d * t
}

impl Vals {
    fn lerp(a: &Vals, b: &Vals, t: f32) -> Vals {
        let l = |x: f32, y: f32| x + (y - x) * t;
        let v3 = |x: [f32; 3], y: [f32; 3]| [l(x[0], y[0]), l(x[1], y[1]), l(x[2], y[2])];
        Vals {
            pos: v3(a.pos, b.pos),
            rot: [turn(a.rot[0], b.rot[0], t), turn(a.rot[1], b.rot[1], t), turn(a.rot[2], b.rot[2], t)],
            size: v3(a.size, b.size),
            scale: v3(a.scale, b.scale),
            color: v3(a.color, b.color),
            round: l(a.round, b.round),
        }
    }
    pub(crate) fn of(s: &crate::model::Shape) -> Vals {
        Vals { pos: s.pos, rot: s.rot, size: s.size, scale: s.scale, color: s.material.color, round: s.round }
    }
}

/// The keys of one shape, in frame order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub uid: u32,
    pub keys: Vec<(u32, Vals)>,
}

/// The keys of one linked copy (found by its group `uid`): where it stands (#126).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkTrack {
    pub uid: u32,
    pub keys: Vec<(u32, Xform)>,
}

fn lerp_xform(a: &Xform, b: &Xform, t: f32) -> Xform {
    let l = |x: f32, y: f32| x + (y - x) * t;
    let v3 = |x: [f32; 3], y: [f32; 3]| [l(x[0], y[0]), l(x[1], y[1]), l(x[2], y[2])];
    Xform {
        pos: v3(a.pos, b.pos),
        rot: [turn(a.rot[0], b.rot[0], t), turn(a.rot[1], b.rot[1], t), turn(a.rot[2], b.rot[2], t)],
        scale: l(a.scale, b.scale),
    }
}

/// How the whole scene melts at a key.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeltKey {
    pub amount: f32,
    pub drip: f32,
    pub ripple: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anim {
    pub fps: u32,
    /// Number of frames (the last frame is `length - 1`).
    pub length: u32,
    /// Hold every pose for a few frames (stop-motion look).
    pub clay: bool,
    pub tracks: Vec<Track>,
    /// Where linked copies stand at each pose (a linked copy has no shapes of its own).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkTrack>,
    pub melt: Vec<(u32, MeltKey)>,
}

impl Default for Anim {
    fn default() -> Self {
        Anim { fps: 24, length: 48, clay: false, tracks: Vec::new(), links: Vec::new(), melt: Vec::new() }
    }
}

fn fin(v: f32, lo: f32, hi: f32, d: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { d }
}

impl Anim {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.links.is_empty() && self.melt.is_empty()
    }

    /// Frames that hold a pose, in order.
    pub fn key_frames(&self) -> Vec<u32> {
        let mut f: Vec<u32> = self.tracks.iter().flat_map(|t| t.keys.iter().map(|k| k.0)).chain(self.links.iter().flat_map(|t| t.keys.iter().map(|k| k.0))).chain(self.melt.iter().map(|k| k.0)).collect();
        f.sort_unstable();
        f.dedup();
        f
    }

    /// Store the scene as it is now at `frame` (the whole scene: every shape, and the melt).
    pub fn capture(&mut self, scene: &mut Scene, frame: u32) -> Result<(), String> {
        let frame = frame.min(self.length.saturating_sub(1));
        if self.key_frames().len() >= MAX_KEYS && !self.key_frames().contains(&frame) {
            return Err(format!("Up to {MAX_KEYS} poses"));
        }
        let ids: Vec<(usize, usize)> = scene.groups.iter().enumerate().flat_map(|(g, gr)| (0..gr.shapes.len()).map(move |s| (g, s))).collect();
        // Callers temporarily take Scene.anim out to avoid borrowing conflicts.
        // Reserve old/deleted shapes' track IDs here, not only in Scene::max_uid.
        let reserved_uid = self.tracks.iter().map(|t| t.uid).max().unwrap_or(0);
        let mut skipped = 0usize;
        for (g, s) in ids {
            let Some(uid) = scene.ensure_shape_uid_above(g, s, reserved_uid) else { continue };
            let Some(shape) = scene.shape(g, s) else { continue };
            let v = Vals::of(shape);
            if self.tracks.iter().all(|t| t.uid != uid) {
                if self.tracks.len() >= MAX_TRACKS {
                    skipped += 1;
                    continue;
                }
                self.tracks.push(Track { uid, keys: Vec::new() });
            }
            if let Some(t) = self.tracks.iter_mut().find(|t| t.uid == uid) {
                put(&mut t.keys, frame, v);
            }
        }
        let linked: Vec<usize> = scene.groups.iter().enumerate().filter(|(_, g)| g.is_linked()).map(|(i, _)| i).collect();
        for gi in linked {
            let reserved = reserved_uid.max(self.links.iter().map(|t| t.uid).max().unwrap_or(0));
            let Some(uid) = scene.ensure_group_uid_above(gi, reserved) else { continue };
            let Some(xform) = scene.groups.get(gi).map(|g| g.xform) else { continue };
            if self.links.iter().all(|t| t.uid != uid) {
                if self.links.len() >= MAX_TRACKS {
                    continue;
                }
                self.links.push(LinkTrack { uid, keys: Vec::new() });
            }
            if let Some(t) = self.links.iter_mut().find(|t| t.uid == uid) {
                put(&mut t.keys, frame, xform);
            }
        }
        put(&mut self.melt, frame, MeltKey { amount: scene.melt.amount, drip: scene.melt.drip, ripple: scene.melt.ripple });
        if skipped > 0 {
            return Err(format!("Only {MAX_TRACKS} shapes can be animated; {skipped} shape(s) were not recorded"));
        }
        Ok(())
    }

    /// Take the pose at `frame` away.
    pub fn remove(&mut self, frame: u32) {
        for t in &mut self.tracks {
            t.keys.retain(|k| k.0 != frame);
        }
        self.tracks.retain(|t| !t.keys.is_empty());
        for t in &mut self.links {
            t.keys.retain(|k| k.0 != frame);
        }
        self.links.retain(|t| !t.keys.is_empty());
        self.melt.retain(|k| k.0 != frame);
    }

    /// Forget every pose.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.links.clear();
        self.melt.clear();
    }

    /// The frame that is really shown at `frame` (with "clay" a pose is held for a few frames).
    pub fn shown(&self, frame: u32) -> u32 {
        if !self.clay {
            return frame;
        }
        let held = frame / CLAY_HOLD * CLAY_HOLD;
        // A pose set between two held frames is still shown on its own frame.
        self.key_frames().into_iter().filter(|k| *k > held && *k <= frame).max().unwrap_or(held)
    }

    /// Put the scene into its pose at `frame`. Shapes without keys stay as they are.
    pub fn apply(&self, scene: &mut Scene, frame: u32) {
        let t = self.shown(frame) as f32;
        for g in &mut scene.groups {
            for s in &mut g.shapes {
                if s.uid == 0 {
                    continue;
                }
                let Some(track) = self.tracks.iter().find(|tr| tr.uid == s.uid) else { continue };
                let Some(v) = sample(&track.keys, t, Vals::lerp) else { continue };
                s.pos = v.pos;
                s.rot = v.rot;
                s.size = v.size;
                s.scale = v.scale;
                s.material.color = v.color;
                s.round = v.round;
            }
        }
        for g in &mut scene.groups {
            if g.uid == 0 || !g.is_linked() {
                continue;
            }
            let Some(track) = self.links.iter().find(|tr| tr.uid == g.uid) else { continue };
            if let Some(x) = sample(&track.keys, t, lerp_xform) {
                g.xform = x;
            }
        }
        if let Some(m) = sample(&self.melt, t, |a: &MeltKey, b: &MeltKey, k| MeltKey {
            amount: a.amount + (b.amount - a.amount) * k,
            drip: a.drip + (b.drip - a.drip) * k,
            ripple: a.ripple + (b.ripple - a.ripple) * k,
        }) {
            scene.melt.amount = m.amount;
            scene.melt.drip = m.drip;
            scene.melt.ripple = m.ripple;
        }
    }

    /// Has the scene been changed since it was posed at `frame`? (Only what moves is compared.)
    pub fn differs(&self, scene: &Scene, frame: u32) -> bool {
        let mut posed = scene.clone();
        self.apply(&mut posed, frame);
        let moved = |a: &Scene, b: &Scene| {
            a.melt != b.melt
                || a.groups.iter().flat_map(|g| &g.shapes).zip(b.groups.iter().flat_map(|g| &g.shapes)).any(|(x, y)| Vals::of(x) != Vals::of(y))
                || a.groups.iter().zip(&b.groups).any(|(x, y)| x.xform != y.xform)
                || a.groups.iter().map(|g| g.shapes.len()).sum::<usize>() != b.groups.iter().map(|g| g.shapes.len()).sum::<usize>()
        };
        moved(scene, &posed)
    }

    /// Keep every number inside what the program copes with (called after reading a file or a command).
    pub fn sanitize(&mut self) {
        self.fps = self.fps.clamp(1, 60);
        self.length = self.length.clamp(1, MAX_FRAMES);
        let last = self.length - 1;
        self.tracks.truncate(MAX_TRACKS);
        for t in &mut self.tracks {
            t.keys.truncate(MAX_KEYS);
            for (f, v) in &mut t.keys {
                *f = (*f).min(last);
                for a in v.pos.iter_mut() {
                    *a = fin(*a, -1.0e5, 1.0e5, 0.0);
                }
                for a in v.rot.iter_mut() {
                    *a = fin(*a, -3600.0, 3600.0, 0.0);
                }
                for a in v.size.iter_mut() {
                    *a = fin(*a, 0.0, 1.0e5, 1.0);
                }
                for a in v.scale.iter_mut() {
                    *a = fin(*a, 0.01, 1.0e3, 1.0);
                }
                for a in v.color.iter_mut() {
                    *a = fin(*a, 0.0, 1.0, 0.5);
                }
                v.round = fin(v.round, 0.0, crate::model::MAX_ROUND, 0.0);
            }
            t.keys.sort_by_key(|k| k.0);
            t.keys.dedup_by_key(|k| k.0);
        }
        self.links.truncate(MAX_TRACKS);
        for t in &mut self.links {
            t.keys.truncate(MAX_KEYS);
            for (f, x) in &mut t.keys {
                *f = (*f).min(last);
                x.sanitize();
            }
            t.keys.sort_by_key(|k| k.0);
            t.keys.dedup_by_key(|k| k.0);
        }
        self.links.retain(|t| !t.keys.is_empty());
        self.melt.truncate(MAX_KEYS);
        for (f, m) in &mut self.melt {
            *f = (*f).min(last);
            m.amount = fin(m.amount, 0.0, 10.0, 1.0);
            m.drip = fin(m.drip, 0.0, 40.0, 0.0);
            m.ripple = fin(m.ripple, 0.3, 4.0, 1.0);
        }
        self.melt.sort_by_key(|k| k.0);
        self.melt.dedup_by_key(|k| k.0);
    }
}

fn put<T>(keys: &mut Vec<(u32, T)>, frame: u32, v: T) {
    match keys.binary_search_by_key(&frame, |k| k.0) {
        Ok(i) => {
            if let Some(k) = keys.get_mut(i) {
                k.1 = v;
            }
        }
        Err(i) => keys.insert(i, (frame, v)),
    }
}

/// The value at time `t` between the keys (held before the first and after the last).
fn sample<T: Copy>(keys: &[(u32, T)], t: f32, mix: impl Fn(&T, &T, f32) -> T) -> Option<T> {
    let first = keys.first()?;
    let last = keys.last()?;
    if t <= first.0 as f32 {
        return Some(first.1);
    }
    if t >= last.0 as f32 {
        return Some(last.1);
    }
    let i = keys.iter().position(|k| k.0 as f32 > t)?;
    let (a, b) = (keys.get(i.checked_sub(1)?)?, keys.get(i)?);
    let span = (b.0 - a.0) as f32;
    Some(mix(&a.1, &b.1, if span > 0.0 { (t - a.0 as f32) / span } else { 1.0 }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rotation_takes_the_short_way_and_spins_stay() {
        assert!((turn(350.0, 10.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((turn(10.0, 350.0, 0.5) - 0.0).abs() < 1e-3);
        assert!((turn(0.0, 720.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((turn(0.0, 90.0, 0.5) - 45.0).abs() < 1e-3);
    }

    #[test]
    fn a_linked_copy_moves_with_its_poses() {
        use crate::model::Group;
        let mut sc = Scene::default();
        let mut src = Group::new("src");
        src.uid = 0;
        sc.groups.push(src);
        let uid = sc.ensure_group_uid(0).expect("uid");
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        sc.groups.push(copy);
        let at = sc.groups.len() - 1;
        let mut a = Anim::default();
        a.capture(&mut sc, 0).expect("pose 0");
        sc.groups[at].xform.pos = [100.0, 0.0, 0.0];
        sc.groups[at].xform.rot = [0.0, 0.0, 90.0];
        assert!(a.differs(&sc, 0), "moving a linked copy is an edit (#126)");
        a.capture(&mut sc, 10).expect("pose 10");
        assert_eq!(a.key_frames(), [0, 10]);
        a.apply(&mut sc, 0);
        assert_eq!(sc.groups[at].xform.pos, [0.0; 3]);
        a.apply(&mut sc, 5);
        assert!((sc.groups[at].xform.pos[0] - 50.0).abs() < 1e-3 && (sc.groups[at].xform.rot[2] - 45.0).abs() < 1e-3);
        a.apply(&mut sc, 10);
        assert_eq!(sc.groups[at].xform.pos, [100.0, 0.0, 0.0]);
        let back: Anim = serde_json::from_str(&serde_json::to_string(&a).expect("save")).expect("load");
        assert_eq!(back, a);
        a.remove(10);
        assert_eq!(a.key_frames(), [0]);
    }

    #[test]
    fn scale_never_reaches_zero_or_below() {
        let mut a = Anim::default();
        let mut v = Vals { pos: [0.0; 3], rot: [0.0; 3], size: [1.0; 3], scale: [0.0, -2.0, 1.0], color: [0.5; 3], round: 0.0 };
        v.scale[2] = f32::NAN;
        a.tracks.push(Track { uid: 1, keys: vec![(0, v)] });
        a.sanitize();
        assert!(a.tracks[0].keys[0].1.scale.iter().all(|s| *s >= 0.01 && s.is_finite()));
    }

    #[test]
    fn clay_shows_a_pose_that_is_not_on_a_multiple_of_three() {
        let mut a = Anim { clay: true, length: 30, ..Anim::default() };
        a.melt.push((4, MeltKey { amount: 1.0, drip: 0.0, ripple: 1.0 }));
        assert_eq!(a.shown(4), 4);
        assert_eq!(a.shown(5), 4);
        assert_eq!(a.shown(3), 3);
        assert_eq!(a.shown(6), 6);
    }

    #[test]
    fn too_many_shapes_is_reported_not_ignored() {
        let mut a = Anim::default();
        for uid in 1..=MAX_TRACKS as u32 {
            a.tracks.push(Track { uid, keys: Vec::new() });
        }
        let mut sc = Scene::default();
        sc.groups[0].shapes[0].uid = 100_000;
        assert!(a.capture(&mut sc, 0).is_err());
    }

    use super::*;

    fn ball() -> Scene {
        Scene::default()
    }

    #[test]
    fn poses_are_blended_between_frames_and_held_outside() {
        let mut sc = ball();
        let mut a = Anim::default();
        a.capture(&mut sc, 0).unwrap();
        let x0 = sc.groups[0].shapes[0].pos[0];
        sc.groups[0].shapes[0].pos[0] = x0 + 40.0;
        sc.melt.amount = 3.0;
        a.capture(&mut sc, 20).unwrap();
        let mut at = sc.clone();
        a.apply(&mut at, 10);
        assert!((at.groups[0].shapes[0].pos[0] - (x0 + 20.0)).abs() < 1e-3);
        assert!((at.melt.amount - 2.0).abs() < 1e-3);
        a.apply(&mut at, 47);
        assert!((at.groups[0].shapes[0].pos[0] - (x0 + 40.0)).abs() < 1e-3);
        a.apply(&mut at, 0);
        assert!((at.groups[0].shapes[0].pos[0] - x0).abs() < 1e-3);
        assert_eq!(a.key_frames(), vec![0, 20]);
    }

    #[test]
    fn a_deleted_shapes_animation_track_is_not_reused_by_a_new_shape() {
        use crate::model::{Kind, Material, Shape};

        let mut scene = Scene::default();
        let mut anim = Anim::default();
        anim.capture(&mut scene, 0).unwrap();
        let old_uid = scene.groups[0].shapes[0].uid;
        scene.groups[0].shapes[0].pos[0] = 100.0;
        anim.capture(&mut scene, 10).unwrap();

        scene.groups[0].shapes.clear();
        scene.groups[0].shapes.push(Shape::new(Kind::Box, [500.0, 0.0, 20.0], Material::default()));
        anim.capture(&mut scene, 20).unwrap();
        let new_uid = scene.groups[0].shapes[0].uid;
        assert!(new_uid > old_uid, "new geometry cannot inherit the deleted shape's track");
        assert_eq!(anim.tracks.len(), 2);
        anim.apply(&mut scene, 0);
        assert_eq!(scene.groups[0].shapes[0].kind, Kind::Box);
        assert_eq!(scene.groups[0].shapes[0].pos[0], 500.0);
        scene.anim = anim;
        let reopened = Scene::from_json(&scene.to_json()).unwrap();
        assert_eq!(reopened.groups[0].shapes[0].uid, new_uid);
        assert_eq!(reopened.anim.tracks.len(), 2);
    }

    #[test]
    fn clay_holds_each_pose() {
        let mut sc = ball();
        let mut a = Anim { clay: true, ..Anim::default() };
        a.capture(&mut sc, 0).unwrap();
        sc.groups[0].shapes[0].pos[0] += 30.0;
        a.capture(&mut sc, 30).unwrap();
        let (mut p, mut q) = (sc.clone(), sc.clone());
        a.apply(&mut p, 10);
        a.apply(&mut q, 11);
        assert_eq!(p.groups[0].shapes[0].pos, q.groups[0].shapes[0].pos);
        a.apply(&mut q, 13);
        assert_ne!(p.groups[0].shapes[0].pos, q.groups[0].shapes[0].pos);
    }

    #[test]
    fn differs_notices_an_edit_and_remove_forgets_a_pose() {
        let mut sc = ball();
        let mut a = Anim::default();
        a.capture(&mut sc, 0).unwrap();
        assert!(!a.differs(&sc, 0));
        sc.groups[0].shapes[0].pos[2] += 5.0;
        assert!(a.differs(&sc, 0));
        a.remove(0);
        assert!(a.is_empty());
    }

    #[test]
    fn hostile_numbers_are_tamed() {
        let mut a = Anim { fps: 0, length: u32::MAX, ..Anim::default() };
        a.tracks.push(Track {
            uid: 1,
            keys: vec![(
                u32::MAX,
                Vals {
                    pos: [f32::NAN, 1e30, 0.0],
                    rot: [0.0; 3],
                    size: [f32::INFINITY, 1.0, 1.0],
                    scale: [1.0; 3],
                    color: [9.0, 0.0, 0.0],
                    round: -4.0,
                },
            )],
        });
        a.sanitize();
        assert_eq!((a.fps, a.length), (1, MAX_FRAMES));
        let v = a.tracks[0].keys[0];
        assert!(
            v.0 < MAX_FRAMES && v.1.pos.iter().chain(&v.1.size).all(|x| x.is_finite() && x.abs() <= 1.0e5) && v.1.color[0] == 1.0 && v.1.round == 0.0
        );
    }
}
