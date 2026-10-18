//! A headless picture of a scene for people (and agents) without a graphics card:
//! `meltcad --render model.yura sheet.png [size]` draws four views (3/4, front, right, top) side by side
//! with the CPU distance field and exits. Flat shading only; the real viewport looks nicer.

use crate::camera::Camera;
use crate::model::Scene;
use crate::sdf::{Field, add, dot, mul, norm, sub};

fn shade(f: &Field, o: [f32; 3], d: [f32; 3]) -> [f32; 3] {
    match f.ray(o, d, 4000.0) {
        Some((t, s)) => {
            let p = add(o, mul(d, t));
            let e = 0.15;
            let n = norm([
                f.distance(add(p, [e, 0.0, 0.0])) - f.distance(sub(p, [e, 0.0, 0.0])),
                f.distance(add(p, [0.0, e, 0.0])) - f.distance(sub(p, [0.0, e, 0.0])),
                f.distance(add(p, [0.0, 0.0, e])) - f.distance(sub(p, [0.0, 0.0, e])),
            ]);
            let key = dot(n, norm([0.5, -0.6, 0.8])).max(0.0);
            let fill = dot(n, norm([-0.7, 0.2, 0.3])).max(0.0) * 0.25;
            let rim = (1.0 - dot(n, mul(d, -1.0)).max(0.0)).powi(3) * 0.15;
            let l = 0.22 + key * 0.7 + fill + rim;
            [s.color[0] * l, s.color[1] * l, s.color[2] * l]
        }
        None => {
            let g = 0.26 + 0.06 * d[2].abs();
            [g, g, g + 0.02]
        }
    }
}

/// Render the four views into one RGBA sheet (2 x size wide and high).
pub fn sheet(scene: &Scene, size: usize) -> (usize, Vec<u8>) {
    let f = Field::new(scene);
    sheet_with_field(scene, &f, size)
}

/// Shared by the public cold renderer and exact interactive-cache image tests.
fn sheet_with_field(scene: &Scene, f: &Field, size: usize) -> (usize, Vec<u8>) {
    let size = size.clamp(40, 600);
    let (c, r) = crate::gpu::bounds(scene, None);
    let (c, r) = if r > 0.0 { (c, r) } else { ([0.0, 0.0, 20.0], 40.0) };
    let views = [(0.6_f32, 0.42_f32), (0.0, 0.0), (std::f32::consts::FRAC_PI_2, 0.0), (0.0, std::f32::consts::FRAC_PI_2)];
    let w = size * 2;
    let mut img = vec![0u8; w * w * 4];
    let tiles: Vec<Vec<u8>> = std::thread::scope(|sc| {
        let hs: Vec<_> = views
            .iter()
            .map(|&(yaw, pitch)| {
                sc.spawn(move || {
                    let mut cam = Camera::default();
                    cam.look(yaw, pitch);
                    cam.frame(c, r);
                    let mut t = vec![0u8; size * size * 4];
                    for y in 0..size {
                        for x in 0..size {
                            let nx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                            let ny = 1.0 - (y as f32 + 0.5) / size as f32 * 2.0;
                            let (o, d) = cam.ray([nx, ny], 1.0);
                            let col = shade(f, o, d);
                            let i = (y * size + x) * 4;
                            for k in 0..3 {
                                t[i + k] = (col[k].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                            }
                            t[i + 3] = 255;
                        }
                    }
                    t
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for (k, t) in tiles.iter().enumerate() {
        let (ox, oy) = ((k % 2) * size, (k / 2) * size);
        for y in 0..size {
            let src = y * size * 4;
            let dst = ((oy + y) * w + ox) * 4;
            if let (Some(s), Some(d)) = (t.get(src..src + size * 4), img.get_mut(dst..dst + size * 4)) {
                d.copy_from_slice(s);
            }
        }
    }
    (w, img)
}

/// One small 3/4 picture of a scene (RGBA, `size` x `size`), for the library's thumbnails.
pub fn thumb(scene: &Scene, size: usize) -> Vec<u8> {
    let size = size.clamp(16, 256);
    let f = Field::new(scene);
    let (c, r) = crate::gpu::bounds(scene, None);
    let (c, r) = if r > 0.0 { (c, r) } else { ([0.0, 0.0, 20.0], 40.0) };
    let mut cam = Camera::default();
    cam.look(0.6, 0.42);
    cam.frame(c, r * 0.62);
    let mut img = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let nx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let ny = 1.0 - (y as f32 + 0.5) / size as f32 * 2.0;
            let (o, d) = cam.ray([nx, ny], 1.0);
            let col = shade(&f, o, d);
            let i = (y * size + x) * 4;
            for k in 0..3 {
                img[i + k] = (col[k].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
            }
            img[i + 3] = 255;
        }
    }
    img
}

/// Handles `--render in.yura out.png [size]`; true when it did (the caller then exits).
pub fn cli(args: &[std::ffi::OsString]) -> bool {
    if args.get(1).and_then(|a| a.to_str()) != Some("--render") {
        return false;
    }
    let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: meltcad --render in.yura out.png [size]");
        return true;
    };
    let size = args.get(4).and_then(|a| a.to_str()).and_then(|a| a.parse().ok()).unwrap_or(220);
    let scene = match std::fs::read_to_string(input).map_err(|e| e.to_string()).and_then(|t| Scene::from_json(&t)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read the scene: {e}");
            return true;
        }
    };
    let (w, rgba) = sheet(&scene, size);
    match crate::png::encode_rgba(w as u32, w as u32, &rgba) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(output, bytes) {
                eprintln!("cannot write the picture: {e}");
            }
        }
        Err(e) => eprintln!("cannot encode the picture: {e}"),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_field_four_view_pixels_match_cold_geometry_materials_and_undo() {
        use crate::model::{BlendMode, Group, Kind, Material, Op, Shape};
        let mut original = Scene::default();
        original.groups.clear();
        let mut hollow = Group::new("thin curved vessel with cut");
        let mut wall = Shape::new(Kind::Sphere, [0.0, 0.0, 14.0], Material::default());
        wall.size = [13.0; 3];
        wall.shell = 0.8;
        wall.material.color = [0.9, 0.3, 0.1];
        let mut cut = Shape::new(Kind::Sphere, [9.0, 0.0, 16.0], Material::default());
        cut.size = [10.0; 3];
        cut.op = Op::Subtract;
        cut.blend = 1.0;
        hollow.shapes.extend([wall, cut]);
        let mut profile = Group::new("rounded profile");
        let mut extrude = Shape::new(Kind::Extrude, [25.0, 1.0, 10.0], Material::default());
        extrude.outline = vec![[-6.0, -8.0], [7.0, -8.0], [10.0, 5.0], [0.0, 10.0], [-8.0, 4.0]];
        extrude.round = 1.2;
        extrude.material.color = [0.15, 0.35, 0.9];
        profile.shapes.push(extrude);
        let mut tube = Group::new("spline");
        let mut curve = Shape::new(Kind::Curve, [0.0, 25.0, 8.0], Material::default());
        curve.path = vec![[0.0, 0.0, 0.0, 2.0], [8.0, -4.0, 2.0, 2.0], [16.0, 5.0, 4.0, 2.0], [24.0, 0.0, 6.0, 2.0]];
        tube.shapes.push(curve);
        original.groups.extend([hollow, profile, tube]);
        original.sanitize();
        let mut previous = Field::reusing(&original, None);
        let mut scene = original.clone();
        let output = std::env::var_os("MELT3D_CACHE_GOLDEN_DIR").map(std::path::PathBuf::from);
        for phase in ["geometry", "material_blend", "undo"] {
            match phase {
                "geometry" => {
                    scene.groups[0].shapes[1].pos[0] += 1.3;
                    scene.groups[2].shapes[0].path[1][1] += 2.5;
                }
                "material_blend" => {
                    scene.groups[0].shapes[0].material.color = [0.1, 0.85, 0.3];
                    scene.groups[1].shapes[0].material.gradient = 0.8;
                    scene.groups[1].shapes[0].material.color2 = [0.9, 0.65, 0.1];
                    scene.melt.mode = BlendMode::Round;
                    scene.melt.amount = 1.4;
                    scene.groups[1].blend = 3.0;
                }
                "undo" => scene = original.clone(),
                _ => unreachable!(),
            }
            let reused = Field::reusing(&scene, Some(&previous));
            let cold = Field::new(&scene);
            let (width, expected) = sheet_with_field(&scene, &cold, 160);
            let (actual_width, actual) = sheet_with_field(&scene, &reused, 160);
            assert_eq!(width, actual_width);
            assert_eq!(actual, expected, "all four views must match exactly after {phase}");
            if let Some(directory) = &output {
                std::fs::create_dir_all(directory).unwrap();
                std::fs::write(directory.join(format!("cache_{phase}.yura")), scene.to_json()).unwrap();
                for (label, pixels) in [("cold", &expected), ("reused", &actual)] {
                    let png = crate::png::encode_rgba(width as u32, width as u32, pixels).unwrap();
                    std::fs::write(directory.join(format!("cache_{phase}_{label}.png")), png).unwrap();
                }
            }
            previous = reused;
        }
    }

    #[test]
    fn the_default_scene_draws_something() {
        let (w, img) = sheet(&Scene::default(), 40);
        assert_eq!(img.len(), w * w * 4);
        // not just one flat colour
        let first = [img[0], img[1], img[2]];
        assert!(img.chunks(4).any(|p| [p[0], p[1], p[2]] != first));
    }
}

/// Paired with the same harness on the base revision to detect sampling regressions that
/// preparation-only cache benchmarks would miss. Sizes and tolerances are identical in both.
#[cfg(test)]
mod cache_runtime_bench {
    use super::*;
    use crate::model::{Group, Kind, Material, Shape};
    use std::time::Instant;

    fn scene(kind: Kind, count: usize) -> Scene {
        let mut scene = Scene::default();
        scene.groups.clear();
        for first in (0..count).step_by(10) {
            let mut group = Group::new(format!("runtime {}", first / 10));
            for i in first..(first + 10).min(count) {
                let mut shape = Shape::new(kind, [(i % 10) as f32 * 18.0, (i / 10) as f32 * 18.0, 8.0], Material::default());
                shape.size = [6.0, 9.0, 6.0];
                shape.blend = 0.6;
                if kind == Kind::Extrude {
                    shape.outline = (0..24)
                        .map(|j| {
                            let a = j as f32 * std::f32::consts::TAU / 24.0;
                            [a.cos() * 5.0, a.sin() * 5.0]
                        })
                        .collect();
                    shape.round = 0.8;
                } else if kind == Kind::Curve {
                    shape.path = (0..8).map(|j| [j as f32 * 1.5, (j as f32 * 0.8).sin() * 2.0, 0.0, 1.0]).collect();
                }
                group.shapes.push(shape);
            }
            scene.groups.push(group);
        }
        scene.sanitize();
        scene
    }

    fn hash(bytes: impl Iterator<Item = u8>) -> u64 {
        bytes.fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
    }

    #[test]
    #[ignore = "paired baseline/candidate CPU runtime measurement"]
    fn cold_sampling_render_and_export_runtime() {
        use crate::export_job::{Lighten, Options, build};
        use crate::export_mesh::Progress;
        for (label, kind) in [("spheres", Kind::Sphere), ("profiles24", Kind::Extrude), ("curves8", Kind::Curve)] {
            let scene = scene(kind, 30);
            let field = Field::new(&scene);
            let points: Vec<_> = scene
                .groups
                .iter()
                .flat_map(|group| &group.shapes)
                .flat_map(|shape| {
                    (0..128).map(move |j| {
                        [shape.pos[0] + (j % 11) as f32 - 5.2, shape.pos[1] + (j % 13) as f32 - 6.3, shape.pos[2] + (j % 17) as f32 - 8.1]
                    })
                })
                .collect();
            let mut samples = vec![0.0; points.len()];
            let start = Instant::now();
            for _ in 0..20 {
                for (out, p) in samples.iter_mut().zip(&points) {
                    *out = field.distance(std::hint::black_box(*p));
                }
                std::hint::black_box(&samples);
            }
            let distance_ns = start.elapsed().as_secs_f64() * 1e9 / (20 * points.len()) as f64;
            let sample_hash = hash(samples.iter().flat_map(|v| v.to_bits().to_le_bytes()));
            let start = Instant::now();
            let (_, pixels) = sheet(&scene, 128);
            let sheet_ms = start.elapsed().as_secs_f64() * 1e3;
            let sheet_hash = hash(pixels.iter().copied());
            let start = Instant::now();
            let pixels = thumb(&scene, 128);
            let thumb_ms = start.elapsed().as_secs_f64() * 1e3;
            let thumb_hash = hash(pixels.iter().copied());
            let small = self::scene(kind, 6);
            let options = Options { tolerance: 0.6, lighten: Lighten::Off, colour: false, ..Options::default() };
            let start = Instant::now();
            let mesh = build(&small, None, &options, &Progress::new()).unwrap();
            let export_ms = start.elapsed().as_secs_f64() * 1e3;
            let mesh_hash = hash(
                mesh.surface
                    .mesh
                    .positions
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_bits().to_le_bytes())
                    .chain(mesh.surface.mesh.triangles.iter().flatten().flat_map(|v| v.to_le_bytes())),
            );
            assert!(mesh.closed);
            eprintln!(
                "RUNTIME {label} samples={} distance_ns={distance_ns:.3} sheet_ms={sheet_ms:.3} thumb_ms={thumb_ms:.3} export_ms={export_ms:.3} sample_hash={sample_hash:016x} sheet_hash={sheet_hash:016x} thumb_hash={thumb_hash:016x} mesh_hash={mesh_hash:016x} vertices={} triangles={}",
                points.len(),
                mesh.verts,
                mesh.tris
            );
        }
    }
}
