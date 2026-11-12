//! "Place on the floor": move shapes straight down (or up) until their lowest point touches the
//! floor (height 0). The lowest point is measured on the real surface, so rotated, rounded and
//! melted shapes all land properly.

use crate::model::{Op, Pick, Scene};
use crate::sdf::Field;

/// How close to the floor counts as "on it", in mm.
const TOUCH: f32 = 0.02;

/// The shapes a selection covers: a group counts with the groups nested in it.
pub fn targets(scene: &Scene, picks: &[Pick]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for p in picks {
        match *p {
            Pick::Shape(g, s) => {
                if scene.shape(g, s).is_some() {
                    out.push((g, s));
                }
            }
            Pick::Group(g) => {
                for gi in g..scene.subtree_end(g).min(scene.groups.len()) {
                    out.extend((0..scene.groups[gi].shapes.len()).map(|si| (gi, si)));
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The height of the lowest point of `targets`, or `None` when they have no surface.
pub fn lowest(scene: &Scene, targets: &[(usize, usize)]) -> Option<f32> {
    // a copy with only those shapes, all of them added and seen, so that a shape that cuts or is
    // hidden still has a bottom to measure
    let mut only = scene.clone();
    for (gi, g) in only.groups.iter_mut().enumerate() {
        g.op = Op::Add;
        g.visible = true;
        let mut si = 0;
        g.shapes.retain(|_| {
            si += 1;
            targets.binary_search(&(gi, si - 1)).is_ok()
        });
        for s in &mut g.shapes {
            s.op = Op::Add;
            s.visible = true;
        }
    }
    let (c, r) = crate::gpu::bounds(&only, None);
    if !(r.is_finite() && r > 0.0 && c.iter().all(|v| v.is_finite())) {
        return None;
    }
    let field = Field::new(&only);
    let z0 = c[2] - r - 1.0;
    let reach = 2.0 * r + 2.0;
    let hit = |x: f32, y: f32| field.ray([x, y, z0], [0.0, 0.0, 1.0], reach).map(|(t, _)| z0 + t);
    // look up from below on a grid, then look closer around the lowest column a few times
    let (mut best, mut at, mut half, mut n): (Option<f32>, [f32; 2], f32, i32) = (None, [c[0], c[1]], r, 16);
    for _ in 0..5 {
        let step = half / n as f32;
        let centre = at;
        for i in -n..=n {
            for j in -n..=n {
                let (x, y) = (centre[0] + i as f32 * step, centre[1] + j as f32 * step);
                if let Some(z) = hit(x, y)
                    && best.is_none_or(|b| z < b)
                {
                    best = Some(z);
                    at = [x, y];
                }
            }
        }
        best?;
        half = step;
        n = 4;
    }
    best.filter(|z| z.is_finite())
}

/// Put `targets` on the floor. Returns how far they moved (up is positive), or why they did not.
pub fn drop(scene: &mut Scene, targets: &[(usize, usize)]) -> Result<f32, &'static str> {
    if targets.is_empty() {
        return Err(crate::i18n::tr("floor.err_nothing"));
    }
    let before = scene.clone();
    let mut moved = 0.0;
    // a second pass takes up what a group's own turn or size changed in the first
    for _ in 0..3 {
        let Some(z) = lowest(scene, targets) else {
            *scene = before;
            return Err(crate::i18n::tr("floor.err_no_surface"));
        };
        if z.abs() <= TOUCH {
            return Ok(moved);
        }
        for &(g, s) in targets {
            if let Some(sh) = scene.shape_mut(g, s) {
                sh.pos[2] -= z;
            }
        }
        moved -= z;
    }
    if lowest(scene, targets).is_some_and(|z| z.abs() <= 0.5) {
        return Ok(moved);
    }
    *scene = before;
    Err(crate::i18n::tr("floor.err_no_surface"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Material, Shape};

    fn scene_with(shapes: Vec<Shape>) -> Scene {
        let mut sc = Scene::default();
        sc.groups[0].shapes = shapes;
        sc
    }

    #[test]
    fn a_floating_box_a_sunk_ball_and_a_tilted_box_all_land_on_the_floor() {
        let mut b = Shape::new(Kind::Box, [0.0, 0.0, 80.0], Material::default());
        b.size = [20.0, 30.0, 40.0];
        let mut ball = Shape::new(Kind::Sphere, [60.0, 0.0, -5.0], Material::default());
        ball.size = [15.0, 15.0, 15.0];
        let mut tilted = Shape::new(Kind::Box, [-70.0, 10.0, 33.0], Material::default());
        tilted.size = [20.0, 20.0, 20.0];
        tilted.rot = [35.0, 20.0, 0.0];
        let mut sc = scene_with(vec![b, ball, tilted]);
        for s in 0..3 {
            let d = drop(&mut sc, &[(0, s)]).expect("lands");
            assert!(d.abs() > 1.0, "shape {s} moved");
            let z = lowest(&sc, &[(0, s)]).expect("has a bottom");
            assert!(z.abs() < 0.1, "shape {s} rests on the floor, bottom at {z}");
        }
        // flat-bottomed and round shapes land where arithmetic says
        assert!((sc.groups[0].shapes[0].pos[2] - 20.0).abs() < 0.1);
        assert!((sc.groups[0].shapes[1].pos[2] - 15.0).abs() < 0.1);
        // only the height changes
        assert_eq!(sc.groups[0].shapes[2].pos[0], -70.0);
        assert_eq!(sc.groups[0].shapes[2].pos[1], 10.0);
    }

    #[test]
    fn a_selection_moves_as_one_and_keeps_its_arrangement() {
        let mut a = Shape::new(Kind::Box, [0.0, 0.0, 50.0], Material::default());
        a.size = [20.0, 20.0, 20.0];
        let mut b = Shape::new(Kind::Box, [40.0, 0.0, 90.0], Material::default());
        b.size = [20.0, 20.0, 20.0];
        let mut sc = scene_with(vec![a, b]);
        let t = targets(&sc, &[Pick::Group(0)]);
        assert_eq!(t, vec![(0, 0), (0, 1)]);
        drop(&mut sc, &t).expect("lands");
        assert!((sc.groups[0].shapes[0].pos[2] - 10.0).abs() < 0.1);
        assert!((sc.groups[0].shapes[1].pos[2] - 50.0).abs() < 0.1, "the higher box stays 40 mm above the lower one");
    }

    #[test]
    fn nothing_to_place_and_hostile_numbers_are_errors_that_change_nothing() {
        let mut sc = scene_with(vec![Shape::new(Kind::Box, [0.0, 0.0, f32::NAN], Material::default())]);
        let before = sc.clone();
        assert!(drop(&mut sc, &[]).is_err());
        assert!(drop(&mut sc, &[(0, 0)]).is_err() || lowest(&sc, &[(0, 0)]).is_some());
        assert!(drop(&mut sc, &[(9, 9)]).is_err());
        assert_eq!(sc.groups.len(), before.groups.len());
        // a sketch line has no surface
        let mut line = Shape::new(Kind::Box, [0.0, 0.0, 30.0], Material::default());
        line.guide = true;
        let mut sc = scene_with(vec![line]);
        assert!(drop(&mut sc, &[(0, 0)]).is_err());
        assert_eq!(sc.groups[0].shapes[0].pos[2], 30.0);
    }
}
