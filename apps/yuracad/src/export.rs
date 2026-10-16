//! Export, from the scene to files. The pieces:
//!
//! - `export_mesh`: the sparse, multi-threaded dual-contouring mesher (the precision is the cell size)
//! - `export_decimate`: quadric-error lightening (flat faces into big triangles) and smoothing
//! - `export_formats`: the writers (STL, OBJ+MTL, 3MF, PLY, GLB), with a small ZIP and DEFLATE
//! - `export_job`: the scene part to export, colours and stains, units and axes, the worker thread
//!   with progress and cancel, the print check and the estimate
//! - `export_dialog`: the card
//!
//! The tests here run the whole chain on scenes (the pieces have their own).

#[cfg(test)]
mod tests {
    use crate::export_formats::Format;
    use crate::export_job::{Lighten, Options, build};
    use crate::export_mesh::Progress;
    use crate::model::{Group, Kind, Material, Op, Scene, Shape};
    use solidcraft_geom::melt;

    fn scene_of(shapes: Vec<Shape>) -> Scene {
        let mut g = Group::new("g");
        g.shapes = shapes;
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() }
    }

    fn slab(x: f32) -> Shape {
        let mut s = Shape::new(Kind::Box, [x, 0.0, 2.0], Material::default());
        s.size = [20.0, 20.0, 4.0];
        s.blend = 0.0;
        s
    }

    fn opts(tol: f32) -> Options {
        Options { format: Format::Stl, tolerance: tol, ..Options::default() }
    }

    #[test]
    fn a_blended_pair_holds_less_than_two_and_more_than_one_and_a_cut_takes_material_away() {
        let ball = |x: f32| {
            let mut s = Shape::new(Kind::Sphere, [x, 0.0, 20.0], Material::default());
            s.blend = 6.0;
            s
        };
        let vol = |sc: &Scene| melt::volume(&build(sc, None, &opts(0.4), &Progress::new()).unwrap().surface.mesh);
        let one = vol(&scene_of(vec![ball(0.0)]));
        let two = vol(&scene_of(vec![ball(-12.5), ball(12.5)]));
        assert!(two < 2.0 * one && two > one, "{two} vs {one}");
        let mut cut = ball(18.0);
        cut.op = Op::Subtract;
        cut.size[0] = 10.0;
        let carved = vol(&scene_of(vec![ball(0.0), cut]));
        assert!(carved < one - 300.0, "{carved} vs {one}");
    }

    #[test]
    fn a_thin_gap_between_two_slabs_survives_every_setting() {
        // two flat slabs 0.6 mm apart (a print-in-place gap)
        let sc = scene_of(vec![slab(-10.3), slab(10.3)]);
        for (tol, lighten) in [(0.1, Lighten::Off), (0.1, Lighten::Flat), (0.2, Lighten::Flat), (0.1, Lighten::Target(500))] {
            let b = build(&sc, None, &Options { lighten, ..opts(tol) }, &Progress::new()).unwrap();
            let v = melt::volume(&b.surface.mesh);
            assert!(b.closed);
            assert!((v / 3200.0 - 1.0).abs() < 0.005, "tolerance {tol} {lighten:?}: volume {v}: the gap was filled in");
        }
    }

    #[test]
    fn decimation_keeps_the_volume_and_a_flat_part_gets_very_light() {
        let mut sc = scene_of(vec![slab(0.0)]);
        sc.groups[0].shapes[0].round = 1.0;
        let full = build(&sc, None, &Options { lighten: Lighten::Off, ..opts(0.1) }, &Progress::new()).unwrap();
        let flat = build(&sc, None, &Options { lighten: Lighten::Flat, ..opts(0.1) }, &Progress::new()).unwrap();
        let (v0, v1) = (melt::volume(&full.surface.mesh), melt::volume(&flat.surface.mesh));
        assert!((v1 / v0 - 1.0).abs() < 0.005, "{v1} vs {v0}");
        assert!(flat.tris * 20 < full.tris, "{} vs {}", flat.tris, full.tris);
        assert!(flat.closed);
        // the same input gives the same file, twice
        let again = build(&sc, None, &Options { lighten: Lighten::Flat, ..opts(0.1) }, &Progress::new()).unwrap();
        assert_eq!(again.surface.mesh, flat.surface.mesh);
    }

    /// Writes one file of every kind into `YURA_DUMP_DIR` to check them with outside tools:
    /// `YURA_DUMP_DIR=/tmp/yura cargo test -p yuracad --release dump_files -- --ignored`, then
    /// `python3 -c "import zipfile, struct, json ..."` on them.
    #[test]
    #[ignore]
    fn dump_files_for_an_outside_check() {
        let Some(dir) = std::env::var_os("YURA_DUMP_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut red = Shape::new(Kind::Sphere, [-12.0, 0.0, 10.0], Material::plain([1.0, 0.1, 0.1], 0.3, 0.0));
        red.size[0] = 10.0;
        let mut blue = Shape::new(Kind::Box, [12.0, 0.0, 10.0], Material::plain([0.1, 0.2, 1.0], 0.8, 1.0));
        blue.size = [16.0, 16.0, 16.0];
        blue.blend = 0.0;
        red.blend = 0.0;
        let sc = scene_of(vec![red, blue]);
        for f in Format::ALL {
            let o = Options { format: f, tolerance: 0.4, units: crate::export_formats::Units::Cm, ..Options::default() };
            let path = dir.join(format!("model.{}", f.ext()));
            let b = build(&sc, None, &o, &Progress::new()).unwrap();
            for (p, data) in crate::export_job::serialize(&b, &o, &path).unwrap() {
                std::fs::write(p, data).unwrap();
            }
        }
    }
}
