//! A headless picture of a scene for people (and agents) without a graphics card:
//! `yuracad --render model.yura sheet.png [size]` draws four views (3/4, front, right, top) side by side
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

/// Handles `--render in.yura out.png [size]`; true when it did (the caller then exits).
pub fn cli(args: &[std::ffi::OsString]) -> bool {
    if args.get(1).and_then(|a| a.to_str()) != Some("--render") {
        return false;
    }
    let (Some(input), Some(output)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: yuracad --render in.yura out.png [size]");
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
    fn the_default_scene_draws_something() {
        let (w, img) = sheet(&Scene::default(), 40);
        assert_eq!(img.len(), w * w * 4);
        // not just one flat colour
        let first = [img[0], img[1], img[2]];
        assert!(img.chunks(4).any(|p| [p[0], p[1], p[2]] != first));
    }
}
