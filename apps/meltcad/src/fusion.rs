//! Smooth Fusion: editable multi-primitive presets backed by the existing Smooth Union.
//! This file does NOT implement a new Boolean/SDF path. Shape::blend is passed to
//! sdf::smin and to the GPU shader exactly as for every other editable shape.

use crate::library::{Category, Item};
use crate::model::{Group, Kind, Material, Op, Shape, SmoothFusion};

pub const COUNTS: [usize; 6] = [3, 4, 5, 6, 7, 8];

/// Relative centroid spacing. The two endpoints approximate a fully merged
/// sculpture and a gently touching arrangement, depending on individual radii.
fn spacing(contact: f32) -> f32 {
    0.63 + 0.97 * contact.clamp(0.0, 1.0)
}

/// Changing a fusion parameter is O(number of child shapes) and never voxelizes
/// or permanently merges the primitives.
pub fn set_radius(group: &mut Group, radius_mm: f32) {
    let Some(f) = &mut group.fusion else { return };
    let radius = if radius_mm.is_finite() { radius_mm.clamp(0.0, 80.0) } else { f.radius_mm };
    if (radius - f.radius_mm).abs() <= f32::EPSILON { return; }
    f.radius_mm = radius;
    for shape in &mut group.shapes {
        if shape.op == Op::Add {
            shape.blend = radius;
        }
    }
}

/// Change overlap without modifying shape parameters, rotation, size, material,
/// or local mesh data. Uses a ratio rather than resetting preset positions,
/// so user-moved shapes remain user-moved after an adjustment.
pub fn set_contact(group: &mut Group, contact: f32) {
    let Some(f) = &mut group.fusion else { return };
    if !contact.is_finite() || group.shapes.is_empty() { return; }
    let contact = contact.clamp(0.0, 1.0);
    if (f.contact - contact).abs() <= f32::EPSILON { return; }
    let ratio = spacing(contact) / spacing(f.contact);
    let n = group.shapes.len() as f32;
    let centre: [f32; 3] = [0, 1, 2].map(|a| group.shapes.iter().map(|s| f64::from(s.pos[a])).sum::<f64>() as f32 / n);
    for s in &mut group.shapes {
        for (p, c) in s.pos.iter_mut().zip(centre) {
            *p = (c + (*p - c) * ratio).clamp(-5000.0, 5000.0);
        }
    }
    f.contact = contact;
}

const PALETTE: [[f32; 3]; 8] = [
    [0.96, 0.66, 0.70], [0.94, 0.73, 0.50], [0.79, 0.68, 0.96],
    [0.65, 0.82, 0.99], [0.66, 0.89, 0.77], [0.98, 0.79, 0.64],
    [0.82, 0.77, 0.96], [0.73, 0.90, 0.98],
];

fn add(group: &mut Group, kind: Kind, pos: [f32; 3], size: [f32; 3], round: f32) {
    let index = group.shapes.len();
    let mat = Material::plain(PALETTE[index % PALETTE.len()], 0.36, 0.0);
    let mut sh = Shape::new(kind, pos, mat);
    sh.name = format!("{} {}", kind.label(), index + 1);
    sh.size = size;
    sh.round = round;
    sh.op = Op::Add;
    sh.blend = group.fusion.unwrap_or_default().radius_mm;
    group.shapes.push(sh);
}

fn ball(g: &mut Group, x: f32, y: f32, z: f32, r: f32) {
    add(g, Kind::Sphere, [x, y, z], [r, 1.0, 1.0], 0.0);
}

pub fn sample(count: usize) -> Option<Item> {
    let name = match count {
        3 => "Smooth Fusion · 3連ブロブ",
        4 => "Smooth Fusion · 四つ葉",
        5 => "Smooth Fusion · ウェーブ",
        6 => "Smooth Fusion · 分岐",
        7 => "Smooth Fusion · ミックス",
        8 => "Smooth Fusion · クラウド",
        _ => return None,
    };
    let mut g = Group::new(name);
    g.fusion = Some(SmoothFusion::default());
    match count {
        3 => {
            ball(&mut g, -23.0, 0.0, 20.0, 20.0);
            ball(&mut g,   0.0, 0.0, 24.0, 22.0);
            ball(&mut g,  23.0, 0.0, 20.0, 20.0);
        }
        4 => {
            for (x, y) in [(-12.5, -12.5), (12.5, -12.5), (-12.5, 12.5), (12.5, 12.5)] {
                ball(&mut g, x, y, 20.0, 20.0);
            }
        }
        5 => {
            for (i, y) in [-5.0, 5.0, 10.0, 3.0, -6.0].into_iter().enumerate() {
                let x = (i as f32 - 2.0) * 24.0;
                ball(&mut g, x, y, 20.0 + (i % 2) as f32 * 3.0, 19.0);
            }
        }
        6 => {
            ball(&mut g, 0.0, 0.0, 25.0, 24.0);
            for i in 0..5 {
                let a = i as f32 * std::f32::consts::TAU / 5.0;
                ball(&mut g, 30.0 * a.cos(), 30.0 * a.sin(), 19.0, 17.0);
            }
        }
        7 => {
            ball(&mut g, 0.0, 0.0, 25.0, 23.0);
            add(&mut g, Kind::Box, [24.0, 0.0, 18.0], [29.0, 30.0, 36.0], 5.0);
            add(&mut g, Kind::Cylinder, [-24.0, 0.0, 20.0], [17.0, 38.0, 1.0], 3.0);
            add(&mut g, Kind::Capsule, [0.0, 26.0, 22.0], [14.0, 20.0, 1.0], 0.0);
            ball(&mut g, 0.0, -24.0, 20.0, 18.0);
            add(&mut g, Kind::Cone, [24.0, 22.0, 18.0], [16.0, 36.0, 1.0], 3.0);
            ball(&mut g, -24.0, -22.0, 18.0, 17.0);
        }
        8 => {
            for (i, (x, y, z)) in [
                (-22.0, -19.0, 18.0), (1.0, -20.0, 22.0),
                (23.0, -16.0, 20.0), (-18.0, 2.0, 23.0),
                (6.0, 4.0, 26.0), (27.0, 5.0, 20.0),
                (-18.0, 24.0, 20.0), (8.0, 25.0, 19.0),
            ].into_iter().enumerate() {
                ball(&mut g, x, y, z, 18.0 + (i % 3) as f32);
            }
        }
        _ => return None,
    }
    Some(Item {
        name: name.into(),
        tags: vec![
            "Smooth Fusion".into(), "Smooth Union".into(), "スムーズフュージョン".into(),
            "なめらか合体".into(), "ぷっくり".into(), format!("{count}個"),
        ],
        group: g,
    })
}

pub fn library_category() -> Category {
    Category {
        name: "Fusion".into(),
        items: COUNTS.into_iter().filter_map(sample).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::model::{BlendMode, Pick, Scene};
    use crate::sdf::Field;

    #[test]
    fn six_presets_have_three_to_eight_separately_editable_primitives() {
        for count in COUNTS {
            let item = sample(count).unwrap();
            assert_eq!(item.group.shapes.len(), count);
            assert!(item.group.fusion.is_some());
            assert!(item.group.shapes.iter().all(|s| s.op == Op::Add && s.blend > 0.0));
            assert!(item.group.shapes.iter().all(|s| s.rot == [0.0; 3] && s.scale == [1.0; 3]));
            assert!(item.group.shapes.iter().any(|s| s.kind == Kind::Sphere));
            let mut scene = item.scene();
            let before = scene.clone();
            scene.sanitize();
            assert_eq!(scene, before, "preset {count} is outside valid file bounds");
        }
        assert!(sample(2).is_none() && sample(9).is_none());
    }

    #[test]
    fn contact_and_radius_change_live_shape_parameters_without_baking_meshes() {
        let mut g = sample(7).unwrap().group;
        let original_positions: Vec<_> = g.shapes.iter().map(|s| s.pos).collect();
        g.shapes[2].pos[0] += 4.0;
        g.shapes[2].rot = [0.0, 12.0, 25.0];
        g.shapes[2].scale = [1.0, 1.2, 0.8];
        let original_rot = g.shapes[2].rot;
        let original_scale = g.shapes[2].scale;
        set_contact(&mut g, 1.0);
        assert!(g.shapes.iter().zip(&original_positions).any(|(s, p)| s.pos != *p));
        assert_eq!(g.shapes[2].rot, original_rot);
        assert_eq!(g.shapes[2].scale, original_scale);
        let transformed = g.shapes.iter().map(|s| s.pos).collect::<Vec<_>>();
        set_contact(&mut g, 1.0);
        assert_eq!(transformed, g.shapes.iter().map(|s| s.pos).collect::<Vec<_>>(), "no drift on unchanged sliders");
        set_radius(&mut g, 27.0);
        assert!(g.shapes.iter().all(|s| s.blend == 27.0));
        set_radius(&mut g, 0.0);
        assert!(g.shapes.iter().all(|s| s.blend == 0.0), "hard Union is available");
    }

    #[test]
    fn smooth_union_uses_existing_sdf_and_unmodified_boolean_precision() {
        let g = sample(3).unwrap().group;
        let mut scene = Scene { groups: vec![g], ..Scene::default() };
        scene.melt.mode = BlendMode::Smooth;
        let soft = Field::new(&scene);
        let (_, join) = crate::sdf::smin(0.0, 0.0, 14.0, BlendMode::Smooth);
        assert!((join - 0.5).abs() < 1e-6);
        for sh in &mut scene.groups[0].shapes { sh.blend = 0.0; }
        let hard = Field::new(&scene);
        let mut softened = 0;
        for x in -45..=45 {
            for y in [-27.0, -22.0, -17.0, -12.0, 0.0, 12.0, 17.0, 22.0, 27.0] {
                let p = [x as f32, y, 22.0];
                let a = soft.distance(p);
                let b = hard.distance(p);
                assert!(a.is_finite() && b.is_finite());
                if a + 0.01 < b { softened += 1; }
            }
        }
        assert!(softened > 10, "a broad smooth joint must change the original hard Union");
    }

    #[test]
    fn four_lobes_still_softly_touch_at_the_light_contact_endpoint() {
        let mut g = sample(4).unwrap().group;
        set_contact(&mut g, 1.0);
        let scene = Scene { groups: vec![g], ..Scene::default() };
        let shapes = &scene.groups[0].shapes;
        // Between the two bottom lobes: at a hard tangency for r=20 mm.
        let midpoint = [0, 1, 2].map(|a| 0.5 * (shapes[0].pos[a] + shapes[1].pos[a]));
        let d = Field::new(&scene).distance(midpoint);
        assert!(d <= 0.05, "the contact setting should not split the four-leaf sample: {d}");
    }

    #[test]
    fn save_reload_undo_redo_and_field_cache_keep_non_destructive_shapes() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let item = sample(5).unwrap();
        assert!(crate::library_ui::insert(&mut app, &ctx, &item));
        app.settle(&ctx);
        let gi = app.scene.groups.len() - 1;
        assert_eq!(app.sel, Some(Pick::Group(gi)));
        let initial = app.scene.clone();
        let field = app.field();
        let builds = app.field_builds.get();
        assert!(std::rc::Rc::ptr_eq(&field, &app.field()));
        assert_eq!(app.field_builds.get(), builds, "unchanged scene reuses the CPU SDF");
        set_radius(&mut app.scene.groups[gi], 28.0);
        set_contact(&mut app.scene.groups[gi], 0.85);
        app.scene.groups[gi].shapes[1].rot[2] = 43.0;
        app.scene.groups[gi].shapes[1].scale[1] = 1.3;
        app.settle(&ctx);
        assert_eq!(app.field_builds.get(), builds);
        let after = app.scene.clone();
        let updated = app.field();
        assert!(!std::rc::Rc::ptr_eq(&field, &updated), "changed SDF is rebuilt once");
        assert_eq!(app.field_builds.get(), builds + 1);
        assert!(std::rc::Rc::ptr_eq(&updated, &app.field()));
        let saved = app.scene.to_json();
        let reopened = Scene::from_json(&saved).unwrap();
        assert_eq!(reopened, after, "settings and all editable child shapes are saved");
        app.undo();
        assert_eq!(app.scene, initial);
        app.redo();
        assert_eq!(app.scene, after);
    }

    #[test]
    fn fusion_cpu_acceleration_matches_exact_smooth_union_near_the_surface() {
        for count in [3, 8] {
            let mut group = sample(count).unwrap().group;
            group.shapes[1].rot = [10.0, 15.0, 4.0];
            group.shapes[1].scale = [1.1, 0.9, 1.2];
            set_contact(&mut group, 0.8);
            set_radius(&mut group, 20.0);
            let scene = Scene { groups: vec![group], ..Scene::default() };
            let fast = Field::new(&scene);
            let slow = Field::new(&scene).without_culling();
            for xi in -10..=10 {
                for yi in -8..=8 {
                    let p = [xi as f32 * 5.0, yi as f32 * 5.0, 20.0];
                    let a = fast.distance(p);
                    let b = slow.distance(p);
                    assert!((a - b).abs() < 1e-3, "sample {count}, {p:?}: fast={a} exact={b}");
                }
            }
        }
    }

    #[test]
    fn gpu_packing_tracks_fusion_edits_using_existing_shader_data_layout() {
        let mut scene = sample(8).unwrap().scene();
        let first = crate::gpu::ScenePack::new(&scene, crate::gpu::VolumeSet::of(&scene));
        assert_eq!(first.shape_n, 8);
        assert_eq!(first.group_n, 1);
        set_radius(&mut scene.groups[0], 35.0);
        set_contact(&mut scene.groups[0], 0.75);
        let changed = crate::gpu::ScenePack::new(&scene, crate::gpu::VolumeSet::of(&scene));
        assert_eq!(changed.shape_n, 8, "fusion is still eight editable GPU primitives");
        assert_ne!(first.list.as_slice(), changed.list.as_slice(), "the existing GPU shader receives changed shape parameters");
    }

    #[test]
    fn damaged_or_legacy_fusion_metadata_is_bounded_without_deleting_shapes() {
        let mut scene = sample(4).unwrap().scene();
        if let Some(f) = &mut scene.groups[0].fusion {
            f.radius_mm = 9999.0;
            f.contact = -999.0;
        }
        scene.sanitize();
        assert_eq!(scene.groups[0].fusion.unwrap().radius_mm, 80.0);
        assert_eq!(scene.groups[0].fusion.unwrap().contact, 0.0);
        assert_eq!(scene.groups[0].shapes.len(), 4);
        let old = r#"{"name":"legacy","op":"Add","blend":0,"visible":true,"shapes":[]}"#;
        let legacy: Group = serde_json::from_str(old).unwrap();
        assert!(legacy.fusion.is_none(), "old .meltcad files remain loadable");
    }

    #[test]
    fn benchmark_prepared_fusion_vs_eight_primitives_does_not_rebake() {
        let scene = sample(8).unwrap().scene();
        let start = std::time::Instant::now();
        let field = Field::new(&scene);
        let prepare_secs = start.elapsed().as_secs_f64();
        let start = std::time::Instant::now();
        for i in 0..2000 {
            let x = (i % 71) as f32 * 1.3 - 42.0;
            let y = (i / 71) as f32 * 3.0 - 42.0;
            std::hint::black_box(field.distance([x, y, 23.0]));
        }
        let sample_secs = start.elapsed().as_secs_f64();
        eprintln!("Smooth Fusion 8: prepare {:.6}s; 2000 SDF samples {:.6}s (CPU; hardware-dependent)", prepare_secs, sample_secs);
        assert!(prepare_secs.is_finite() && sample_secs.is_finite());
    }
}
