//! Editable Organic Tube presets. This is intentionally *not* a new geometry
//! kernel: every preset is an ordinary Kind::Curve, so the existing Catmull-Rom,
//! node/radius gizmos, SDF, undo, serialization and mesh exporter remain in charge.

use std::f32::consts::{PI, TAU};

use crate::app::App;
use crate::model::{Group, Kind, MAX_GROUPS, MAX_SHAPES, Material, Op, Pick, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Wave,
    Ring,
    S,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Wave, Preset::Ring, Preset::S];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Wave => "波型",
            Preset::Ring => "リング型",
            Preset::S => "S字型",
        }
    }

    pub fn search_words(self) -> &'static str {
        match self {
            Preset::Wave => "organic tube wave wavy なみ うねうね",
            Preset::Ring => "organic tube ring loop circle ドーナツ わ",
            Preset::S => "organic tube s shape snake curve エス くねくね",
        }
    }

    /// The preset is a fully editable native curve with radius at every node.
    pub fn shape(self) -> Shape {
        let (name, color) = match self {
            Preset::Wave => ("Organic Tube · Wave", [0.54, 0.79, 0.99]),
            Preset::Ring => ("Organic Tube · Ring", [0.94, 0.62, 0.78]),
            Preset::S => ("Organic Tube · S", [0.65, 0.90, 0.72]),
        };
        let mut sh = Shape::new(Kind::Curve, [0.0, 0.0, 34.0], Material::plain(color, 0.34, 0.0));
        sh.name = name.to_owned();
        sh.blend = 1.0;
        sh.op = Op::Add;
        // params: (closed loop, Catmull-Rom smoothness, flatness of caps, radius multiplier)
        sh.params = [0.0, 1.0, 0.0, 1.0];
        // Curve-specific size: [unused, section width ratio, section contour].
        // 1.0 / 1.0 exactly preserves the old circular SDF.
        sh.size[1] = 1.0;
        sh.size[2] = 1.0;
        sh.path = match self {
            Preset::Wave => (0..=8)
                .map(|i| {
                    let t = i as f32 / 8.0;
                    let x = -65.0 + t * 130.0;
                    let y = 8.0 * (t * TAU).sin();
                    let z = 12.0 * (t * TAU * 1.5).sin();
                    let r = 5.5 + 1.3 * (t * TAU).sin();
                    [x, y, z, r]
                })
                .collect(),
            Preset::Ring => {
                sh.params[0] = 1.0;
                (0..12)
                    .map(|i| {
                        let a = i as f32 * TAU / 12.0;
                        [37.0 * a.cos(), 37.0 * a.sin(), 6.0 * (2.0 * a).sin(), 5.0 + 1.2 * (3.0 * a).cos()]
                    })
                    .collect()
            }
            Preset::S => (0..=8)
                .map(|i| {
                    let t = i as f32 / 8.0;
                    let y = -58.0 + t * 116.0;
                    let x = 29.0 * (2.0 * PI * t).sin();
                    let z = 10.0 * (t * PI).sin();
                    let r = 4.7 + 1.5 * (t * TAU).sin().powi(2);
                    [x, y, z, r]
                })
                .collect(),
        };
        sh
    }
}

impl App {
    /// Add the chosen Organic Tube as its own native curve group. Do not
    /// silently fuse it with an existing selected object.
    pub fn add_organic_tube(&mut self, ctx: &egui::Context, preset: Preset) {
        if self.scene.shape_count() >= MAX_SHAPES || self.scene.groups.len() >= MAX_GROUPS {
            self.say(ctx, "形またはグループの上限です。不要な形を減らしてください");
            return;
        }
        self.finish_drawing();
        self.ct.drawing = false;
        self.ct.draw_shape = None;
        self.sketch_off();
        self.set_pipe_tool(false);
        let mut group = Group::new(format!("Organic Tube · {}", preset.label()));
        group.shapes.push(preset.shape());
        let g = self.scene.groups.len();
        self.scene.groups.push(group);
        self.sel = Some(Pick::Shape(g, 0));
        self.ct.edit_nodes = true;
        self.ct.node = Some((g, 0, 0));
        self.ct.hover = None;
        self.ct.drag = None;
        self.frame_view(true);
        self.say(ctx, format!("Organic Tube {}を追加。点をドラッグ／太さを調整できます", preset.label()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve;
    use crate::sdf::{Field, Prepared};

    #[test]
    fn all_presets_use_existing_editable_curve_and_finite_sdf() {
        for preset in Preset::ALL {
            let sh = preset.shape();
            assert_eq!(sh.kind, Kind::Curve);
            assert!((3..=crate::model::MAX_PATH).contains(&sh.path.len()));
            assert!(sh.path.iter().flatten().all(|x| x.is_finite()));
            assert!(sh.path.iter().all(|x| x[3] >= 3.0));
            assert!(sh.params[1] > 0.9, "smooth by default");
            assert_eq!(sh.params[0] > 0.5, preset == Preset::Ring);
            let points = curve::build(&sh.path, &sh.params);
            assert!(points.len() >= sh.path.len() && points.len() <= curve::MAX_POINTS);
            assert!(points.iter().flatten().all(|v| v.is_finite()));
            if preset == Preset::Ring {
                assert_eq!(points.first(), points.last(), "loop must join");
            }
            let prepared = Prepared::new(&sh);
            for i in -5..=5 {
                assert!(prepared.distance([i as f32 * 12.0, i as f32 * 5.0, 35.0]).is_finite());
            }
        }
    }

    #[test]
    fn preset_nodes_edit_save_and_undo_with_the_native_engine() {
        let ctx = egui::Context::default();
        let mut app = App::new(true, None);
        let before = app.scene.clone();
        app.add_organic_tube(&ctx, Preset::Wave);
        assert_eq!(app.scene.shape_count(), before.shape_count() + 1);
        let Some(Pick::Shape(g, s)) = app.sel else { panic!("preset selected") };
        assert!(app.node_shape().is_some(), "same viewport node gizmo as an ordinary curve");
        app.scene.shape_mut(g, s).expect("tube").path[3][3] = 9.0;
        app.scene.shape_mut(g, s).expect("tube").path[3][2] += 8.0;
        let saved = app.scene.to_json();
        let loaded = crate::model::Scene::from_json(&saved).expect("valid native scene");
        assert_eq!(loaded, app.scene);
        let field = Field::new(&loaded);
        assert!(field.distance([0.0, 0.0, 34.0]).is_finite());
        app.settle(&ctx);
        app.undo();
        assert_eq!(app.scene, before, "preset and edits belong to one normal undo transaction");
        app.redo();
        assert_eq!(app.scene, loaded);
    }

    #[test]
    fn ring_can_be_opened_and_restored_without_changing_nodes() {
        let mut ring = Preset::Ring.shape();
        let nodes = ring.path.clone();
        ring.params[0] = 0.0;
        let open = curve::build(&ring.path, &ring.params);
        assert_ne!(open.first(), open.last());
        ring.params[0] = 1.0;
        let closed = curve::build(&ring.path, &ring.params);
        assert_eq!(closed.first(), closed.last());
        assert_eq!(ring.path, nodes, "opening and closing does not discard control points");
    }
}
