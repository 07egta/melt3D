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

/// One picture of the scene from `cam`, `w` x `h` RGBA (for animation frames).
pub fn view(scene: &Scene, cam: &Camera, w: usize, h: usize) -> Vec<u8> {
    let (w, h) = (w.clamp(16, 1200), h.clamp(16, 1200));
    let f = Field::new(scene);
    let aspect = w as f32 / h as f32;
    let rows: Vec<Vec<u8>> = std::thread::scope(|sc| {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 8);
        let hs: Vec<_> = (0..threads)
            .map(|k| {
                let f = &f;
                sc.spawn(move || {
                    let mut part = Vec::new();
                    for y in (k..h).step_by(threads) {
                        let mut row = vec![0u8; w * 4];
                        for x in 0..w {
                            let nx = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                            let ny = 1.0 - (y as f32 + 0.5) / h as f32 * 2.0;
                            let (o, d) = cam.ray([nx, ny], aspect);
                            let col = shade(f, o, d);
                            for c in 0..3 {
                                row[x * 4 + c] = (col[c].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                            }
                            row[x * 4 + 3] = 255;
                        }
                        part.push((y, row));
                    }
                    part
                })
            })
            .collect();
        let mut rows = vec![Vec::new(); h];
        for hnd in hs {
            for (y, row) in hnd.join().unwrap_or_default() {
                if let Some(slot) = rows.get_mut(y) {
                    *slot = row;
                }
            }
        }
        rows
    });
    rows.concat()
}

/// Render the four views into one RGBA sheet (2 x size wide and high).
pub fn sheet(scene: &Scene, size: usize) -> (usize, Vec<u8>) {
    let size = size.clamp(40, 600);
    let f = Field::new(scene);
    let (c, r) = crate::gpu::bounds(scene, None);
    let (c, r) = if r > 0.0 { (c, r) } else { ([0.0, 0.0, 20.0], 40.0) };
    let views = [(0.6_f32, 0.42_f32), (0.0, 0.0), (std::f32::consts::FRAC_PI_2, 0.0), (0.0, std::f32::consts::FRAC_PI_2)];
    let w = size * 2;
    let mut img = vec![0u8; w * w * 4];
    let tiles: Vec<Vec<u8>> = std::thread::scope(|sc| {
        let hs: Vec<_> = views
            .iter()
            .map(|&(yaw, pitch)| {
                let f = &f;
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

/// Handles `--render in.yura out.png [size]`. `None` means this is
/// another command; `Some(false)` means render failed (non-zero process exit).
pub fn cli(args: &[std::ffi::OsString]) -> Option<bool> {
    if args.get(1).and_then(|a| a.to_str()) != Some("--render") {
        return None;
    }
    let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: meltcad --render in.yura out.png [size]");
        return Some(false);
    };
    // The renderer's sheet() function also clamps this, but keep the CLI
    // explicit about its maximum memory budget.
    let size: usize = args.get(4).and_then(|a| a.to_str()).and_then(|a| a.parse().ok()).unwrap_or(220).clamp(40, 600);
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
    let step = (a.length / 240 + 1).max(a.fps.max(1).div_ceil(50)).max(1);
    let frames: Vec<u32> = (0..a.length).step_by(step as usize).collect();
    let total = frames.len() as u32;
    // the same picture as the window when there is a graphics card to ask (else the plain CPU one)
    let mut gpu = if use_gpu { crate::gpu::offscreen::Offscreen::open_with(None, None).ok() } else { None };
    let pic = gpu.as_ref().map(|g| g.picture([w as u32, h as u32]));
    let mut pics = Vec::new();
    for (i, &f) in frames.iter().enumerate() {
        if !progress(i as u32, total) {
            return Err("Stopped".into());
        }
        let mut s = scene.clone();
        a.apply(&mut s, f);
        match (gpu.as_mut(), pic.as_ref()) {
            (Some(g), Some(pic)) => {
                let pack = crate::gpu::ScenePack::new(&s, crate::gpu::VolumeSet::of(&s));
                g.load(&pack);
                let mut globals = pack.globals(&s, &cam, [w as f32, h as f32], None, None, false);
                crate::gpu::set_view_flags(&mut globals, crate::perf::view_flags(false, false, pack.glow));
                g.frame(pic, &globals, false);
                pics.push(g.read(pic));
            }
            _ => pics.push(view(&s, &cam, w, h)),
        }
    }
    let delays = crate::gif::delays(pics.len(), 100.0 * step as f32 / a.fps.max(1) as f32);
    crate::gif::encode_timed(w, h, &pics, &delays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_cli_reports_read_and_write_errors_in_its_exit_status() {
        let dir = std::env::temp_dir().join(format!("melt3d-render-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("scene.meltcad");
        let png = dir.join("output.png");
        let args = |inp: &std::path::Path, out: &std::path::Path| -> Vec<std::ffi::OsString> {
            vec!["meltcad".into(), "--render".into(), inp.as_os_str().to_os_string(),
                 out.as_os_str().to_os_string(), "40".into()]
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
    fn the_default_scene_draws_something() {
        let (w, img) = sheet(&Scene::default(), 40);
        assert_eq!(img.len(), w * w * 4);
        // not just one flat colour
        let first = [img[0], img[1], img[2]];
        assert!(img.chunks(4).any(|p| [p[0], p[1], p[2]] != first));
    }
}
