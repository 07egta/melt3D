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

/// One picture of the scene from `cam`, `w` x `h` RGBA, for a single picture (tests); a run of
/// pictures draws into a buffer it reuses with [`view_into`].
#[cfg(test)]
pub fn view(scene: &Scene, cam: &Camera, w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::new();
    view_into(scene, cam, w, h, &mut out);
    out
}

/// Draw one picture into `out` (made `w` x `h` x 4 long). The rows are shared between the threads
/// as one buffer, so a whole run of animation pictures can reuse the same allocation (#217).
pub fn view_into(scene: &Scene, cam: &Camera, w: usize, h: usize, out: &mut Vec<u8>) {
    let (w, h) = (w.clamp(16, 1200), h.clamp(16, 1200));
    let f = Field::new(scene);
    let aspect = w as f32 / h as f32;
    out.clear();
    out.resize(w * h * 4, 0);
    std::thread::scope(|sc| {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 8);
        let rows = h.div_ceil(threads).max(1);
        for (k, part) in out.chunks_mut(rows * w * 4).enumerate() {
            let f = &f;
            sc.spawn(move || {
                for (j, row) in part.chunks_mut(w * 4).enumerate() {
                    let y = k * rows + j;
                    for x in 0..w {
                        let nx = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                        let ny = 1.0 - (y as f32 + 0.5) / h as f32 * 2.0;
                        let (o, d) = cam.ray([nx, ny], aspect);
                        let col = shade(f, o, d);
                        for (c, v) in col.iter().enumerate() {
                            if let Some(slot) = row.get_mut(x * 4 + c) {
                                *slot = (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                            }
                        }
                        if let Some(slot) = row.get_mut(x * 4 + 3) {
                            *slot = 255;
                        }
                    }
                }
            });
        }
    });
}

/// Render the four views into one RGBA sheet (2 x size wide and high).
pub fn sheet(scene: &Scene, size: usize) -> (usize, Vec<u8>) {
    let f = Field::new(scene);
    sheet_with_field(scene, &f, size)
}

/// Shared by the public cold renderer and the exact incremental-field image tests.
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
    // Include the geometry bounds in the thumbnail; the previous 0.62 factor
    // cropped long limbs and thin appendages in several library models.
    cam.frame(c, r);
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

/// Keep headless image sizes within the same bounds as the automation API.
fn render_cli_size(value: Option<&std::ffi::OsString>) -> usize {
    value.and_then(|a| a.to_str()).and_then(|a| a.parse::<usize>().ok()).unwrap_or(220).clamp(40, 600)
}

/// Handles `--render in.yura out.png [size]`.
/// None = another command; Some(true) = success; Some(false) = failure.
pub fn cli(args: &[std::ffi::OsString]) -> Option<bool> {
    if args.get(1).and_then(|a| a.to_str()) != Some("--render") {
        return None;
    }
    let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: meltcad --render in.yura out.png [size]");
        return Some(false);
    };
    let size = render_cli_size(args.get(4));
    let scene = match crate::recovery::read_project(std::path::Path::new(input)) {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("cannot read the scene: {e}");
            return Some(false);
        }
    };
    let (w, rgba) = sheet(&scene, size);
    let bytes = match crate::png::encode_rgba(w as u32, w as u32, &rgba) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot encode the picture: {e}");
            return Some(false);
        }
    };
    match std::fs::write(output, bytes) {
        Ok(()) => Some(true),
        Err(e) => {
            eprintln!("cannot write the picture: {e}");
            Some(false)
        }
    }
}

/// The interval of stored GIF pictures. 240 frames must not be thinned at all.
fn gif_frame_step(length: u32, fps: u32) -> u32 {
    (length.saturating_sub(1) / 240 + 1).max(fps.max(1).div_ceil(50))
}

/// Encode `count` pictures as they are drawn. `draw` fills the single picture buffer for picture
/// `i`; that picture is written to the GIF before the next one is drawn, so the loop keeps two
/// full pictures at most (the one being drawn and the writer's 8-bit index picture) instead of
/// all of them (#217). Returns the GIF and the most full pictures that were alive at once.
fn encode_stream(
    w: usize,
    h: usize,
    count: usize,
    mut delay_of: impl FnMut(usize) -> u16,
    mut draw: impl FnMut(usize, &mut Vec<u8>) -> Result<(), String>,
    mut progress: impl FnMut(u32, u32) -> bool,
) -> Result<(Vec<u8>, usize), String> {
    let mut writer = crate::gif::Writer::new(w, h)?;
    let mut pic = vec![0u8; writer.picture_len()];
    let total = count.min(u32::MAX as usize) as u32;
    for i in 0..count {
        if !progress(i as u32, total) {
            return Err("Stopped".into());
        }
        draw(i, &mut pic)?;
        writer.push(&pic, delay_of(i))?;
    }
    if !progress(total, total) {
        return Err("Stopped".into());
    }
    // the picture buffer above, plus the writer's index picture
    let peak = 1 + writer.peak_pictures();
    Ok((writer.finish(), peak))
}

/// The whole animation as a GIF: every frame (or every `step`-th) drawn from `cam`, or from a
/// camera that fits the piece at all its poses. `size` is the width in pixels.
pub fn animation_gif(
    scene: &Scene,
    cam: Option<Camera>,
    size: usize,
    use_gpu: bool,
    mut progress: impl FnMut(u32, u32) -> bool,
) -> Result<Vec<u8>, String> {
    let a = &scene.anim;
    if a.is_empty() {
        return Err("No poses yet. Set a pose first".into());
    }
    let w = size.clamp(64, 640);
    let h = (w * 3 / 4).max(48);
    let cam = cam.unwrap_or_else(|| {
        let keys = a.key_frames();
        let spheres: Vec<([f32; 3], f32)> = keys
            .iter()
            .map(|&f| {
                let mut s = scene.clone();
                a.apply(&mut s, f);
                crate::gpu::bounds(&s, None)
            })
            .filter(|(_, r)| *r > 0.0)
            .collect();
        let n = spheres.len().max(1) as f32;
        let c = spheres.iter().fold([0.0f32; 3], |m, (c, _)| [m[0] + c[0] / n, m[1] + c[1] / n, m[2] + c[2] / n]);
        let r =
            spheres.iter().map(|(p, r)| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt() + r).fold(0.0f32, f32::max);
        let (c, r) = if spheres.is_empty() { ([0.0, 0.0, 20.0], 40.0) } else { (c, r) };
        let mut cam = Camera::default();
        cam.look(0.6, 0.42);
        cam.frame(c, r);
        cam
    });
    // a GIF frame lasts 2 hundredths of a second at the least: above 50 fps, pictures are skipped
    // (and the waits shared out evenly) so the animation still takes as long as it should
    let step = gif_frame_step(a.length, a.fps);
    let frames: Vec<u32> = (0..a.length).step_by(step as usize).collect();
    let delays = crate::gif::delays(frames.len(), 100.0 * step as f32 / a.fps.max(1) as f32);
    // the same picture as the window when there is a graphics card to ask (else the plain CPU one)
    let mut gpu = if use_gpu { crate::gpu::offscreen::Offscreen::open_with(None, None).ok() } else { None };
    let pic = gpu.as_ref().map(|g| g.picture([w as u32, h as u32]));
    let (gif, _peak) = encode_stream(
        w,
        h,
        frames.len(),
        |i| delays.get(i).copied().unwrap_or(2),
        |i, out| {
            let Some(&f) = frames.get(i) else { return Err("Missing frame".into()) };
            let mut s = scene.clone();
            a.apply(&mut s, f);
            match (gpu.as_mut(), pic.as_ref()) {
                (Some(g), Some(pic)) => {
                    let pack = crate::gpu::ScenePack::new(&s, crate::gpu::VolumeSet::of(&s));
                    g.load(&pack);
                    let mut globals = pack.globals(&s, &cam, [w as f32, h as f32], None, None, false);
                    crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, false, pack.glow));
                    g.frame(pic, &globals, false);
                    if out.len() != (w * h * 4) {
                        return Err("Bad picture size".into());
                    }
                    g.read_into(pic, out);
                    Ok(())
                }
                _ => {
                    view_into(&s, &cam, w, h, out);
                    Ok(())
                }
            }
        },
        &mut progress,
    )?;
    // Encoding a complete set of frames may take time; cancellation can arrive
    // while the palette/LZW encoder runs, before the caller writes a file.
    if !progress(frames.len() as u32, frames.len() as u32) {
        return Err("Stopped".into());
    }
    Ok(gif)
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
        let mut previous = (Field::reusing(&original, None), original.clone());
        let mut scene = original.clone();
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
                _ => scene = original.clone(),
            }
            let reused = Field::reusing(&scene, Some((&previous.0, &previous.1)));
            let cold = Field::new(&scene);
            let (width, expected) = sheet_with_field(&scene, &cold, 160);
            let (actual_width, actual) = sheet_with_field(&scene, &reused, 160);
            assert_eq!(width, actual_width);
            assert!(actual == expected, "all four views must match exactly after {phase}");
            previous = (reused, scene.clone());
        }
    }

    #[test]
    fn render_cli_reports_read_and_write_errors_in_its_exit_status() {
        let dir = std::env::temp_dir().join(format!("melt3d-render-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("scene.meltcad");
        let png = dir.join("output.png");
        let args = |inp: &std::path::Path, out: &std::path::Path| -> Vec<std::ffi::OsString> {
            vec!["meltcad".into(), "--render".into(), inp.as_os_str().to_os_string(), out.as_os_str().to_os_string(), "40".into()]
        };
        assert_eq!(cli(&["meltcad".into(), "--other".into()]), None);
        assert_eq!(cli(&args(&dir.join("missing.meltcad"), &png)), Some(false));
        assert!(!png.exists());
        std::fs::write(&input, Scene::default().try_to_json().unwrap()).unwrap();
        assert_eq!(cli(&args(&input, &dir.join("nonexistent-parent").join("out.png"))), Some(false));
        assert_eq!(cli(&args(&input, &png)), Some(true));
        let bytes = std::fs::read(&png).unwrap();
        assert!(bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn gif_240_frame_animation_is_not_halved() {
        assert_eq!(gif_frame_step(239, 30), 1);
        assert_eq!(gif_frame_step(240, 30), 1);
        assert_eq!((0..240u32).step_by(gif_frame_step(240, 30) as usize).next_back(), Some(239));
        assert_eq!(gif_frame_step(241, 30), 2);
        assert_eq!(gif_frame_step(240, 60), 2, "60 fps needs >=2 centiseconds per GIF image");
    }

    #[test]
    fn render_cli_reports_input_errors_and_clamps_untrusted_dimensions() {
        let args = |v: &[&str]| -> Vec<std::ffi::OsString> { v.iter().copied().map(Into::into).collect() };
        assert_eq!(cli(&args(&["meltcad"])), None);
        assert_eq!(cli(&args(&["meltcad", "--render"])), Some(false));
        assert_eq!(cli(&args(&["meltcad", "--render", "nonexistent-scene.yura", "out.png"])), Some(false));
        assert_eq!(render_cli_size(Some(&"999999999".into())), 600);
        assert_eq!(render_cli_size(Some(&"1".into())), 40);
        assert_eq!(render_cli_size(Some(&"bad".into())), 220);
    }

    #[test]
    fn view_into_fills_a_reused_buffer_like_view() {
        // The animation draws into one buffer again and again; it must be the same picture.
        let scene = Scene::default();
        let cam = Camera::default();
        let one = view(&scene, &cam, 24, 20);
        let mut reused = vec![7u8; 100];
        view_into(&scene, &cam, 24, 20, &mut reused);
        assert_eq!(one, reused);
    }

    #[test]
    fn a_long_gif_export_holds_two_pictures_at_most() {
        // 72 pictures at 32 x 24: the run draws into one reused buffer and encodes each picture
        // before drawing the next, so peak memory does not grow with the length (#217).
        let (w, h, n) = (32usize, 24usize, 72usize);
        let mut seen: Vec<usize> = Vec::new();
        let (gif, peak) = encode_stream(
            w,
            h,
            n,
            |i| 2 + (i % 3) as u16,
            |i, pic| {
                seen.push(pic.as_ptr() as usize);
                assert_eq!(pic.len(), w * h * 4);
                for (k, px) in pic.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    *px = [(k % 251) as u8, i as u8, 40, 255];
                }
                Ok(())
            },
            |_, _| true,
        )
        .unwrap();
        assert_eq!(peak, 2, "the picture being drawn and the writer's index picture");
        assert_eq!(seen.len(), n);
        assert!(seen.iter().all(|p| *p == seen[0]), "one picture buffer is reused for the whole run");
        assert!(gif.starts_with(b"GIF89a"));
        assert_eq!(gif.last(), Some(&0x3B));
    }

    #[test]
    fn a_stopped_gif_export_keeps_no_pictures_and_no_output() {
        let mut drawn = 0;
        let r = encode_stream(
            32,
            24,
            50,
            |_| 4,
            |_, pic| {
                drawn += 1;
                pic.fill(0);
                Ok(())
            },
            |i, _| i < 5,
        );
        assert_eq!(r.err().as_deref(), Some("Stopped"));
        assert_eq!(drawn, 5, "stops at the cancelled picture");
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
