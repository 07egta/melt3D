//! A tiny software rasteriser so the look can be checked without a GPU: `cargo test -p meltcad preview -- --ignored`
//! writes `$YURA_PREVIEW` (default yura_preview.ppm). Text is skipped (it needs the font atlas); shapes are exact.

use super::*;
use egui::epaint::{ImageData, Primitive};
use std::collections::HashMap;

pub(crate) type Textures = HashMap<egui::TextureId, (usize, usize, Vec<Color32>)>;

/// Applies the texture changes of a frame (the font atlas) to our copies.
pub(crate) fn apply_deltas(tex: &mut Textures, delta: &egui::TexturesDelta) {
    for (id, list) in &delta.set {
        for d in list {
            let ImageData::Color(img) = &d.image;
            let [iw, ih] = img.size;
            match d.pos {
                None => {
                    tex.insert(*id, (iw, ih, img.pixels.clone()));
                }
                Some([px, py]) => {
                    if let Some((tw, _, pixels)) = tex.get_mut(id) {
                        for y in 0..ih {
                            for x in 0..iw {
                                if let (Some(dst), Some(src)) = (pixels.get_mut((py + y) * *tw + px + x), img.pixels.get(y * iw + x)) {
                                    *dst = *src;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sample(tex: &Textures, id: egui::TextureId, uv: egui::Pos2) -> [f32; 4] {
    let Some((w, h, px)) = tex.get(&id) else { return [1.0; 4] };
    let (x, y) = (uv.x * *w as f32 - 0.5, uv.y * *h as f32 - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xx: f32, yy: f32| {
        let xi = (xx.max(0.0) as usize).min(w - 1);
        let yi = (yy.max(0.0) as usize).min(h - 1);
        let c = px.get(yi * w + xi).copied().unwrap_or(Color32::WHITE);
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]
    };
    let (a, b, c, d) = (at(x0, y0), at(x0 + 1.0, y0), at(x0, y0 + 1.0), at(x0 + 1.0, y0 + 1.0));
    let mut out = [0.0; 4];
    for k in 0..4 {
        out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    out
}

fn blend(dst: &mut [f32; 3], src: [f32; 4]) {
    // premultiplied colour over opaque destination
    for i in 0..3 {
        dst[i] = src[i] + dst[i] * (1.0 - src[3]);
    }
}

pub(crate) fn raster(prims: &[egui::ClippedPrimitive], tex: &Textures, w: usize, h: usize, ppp: f32, bg: [f32; 3]) -> Vec<[f32; 3]> {
    let mut img = vec![bg; w * h];
    for cp in prims {
        let Primitive::Mesh(mesh) = &cp.primitive else { continue };
        let clip = cp.clip_rect;
        for t in mesh.indices.as_chunks::<3>().0 {
            let v = [mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]];
            let p: Vec<(f32, f32)> = v.iter().map(|x| (x.pos.x * ppp, x.pos.y * ppp)).collect();
            let minx = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor().max(clip.left() * ppp).max(0.0) as usize;
            let maxx = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil().min(clip.right() * ppp).min(w as f32 - 1.0)).max(0.0) as usize;
            let miny = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor().max(clip.top() * ppp).max(0.0) as usize;
            let maxy = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil().min(clip.bottom() * ppp).min(h as f32 - 1.0)).max(0.0) as usize;
            let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-6 {
                continue;
            }
            for y in miny..=maxy {
                for x in minx..=maxx {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
                    let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                        continue;
                    }
                    let mut c = [0.0; 4];
                    for k in 0..4 {
                        let ch = |col: Color32| [col.r(), col.g(), col.b(), col.a()][k] as f32 / 255.0;
                        c[k] = w0 * ch(v[0].color) + w1 * ch(v[1].color) + w2 * ch(v[2].color);
                    }
                    let uv = egui::pos2(w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x, w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y);
                    let t = sample(tex, mesh.texture_id, uv);
                    for k in 0..4 {
                        c[k] *= t[k];
                    }
                    if let Some(d) = img.get_mut(y * w + x) {
                        blend(d, c);
                    }
                }
            }
        }
    }
    img
}

#[test]
#[ignore]
fn preview() {
    let ppp = 3.0;
    let ctx = egui::Context::default();
    apply(&ctx);
    crate::fonts::setup(&ctx);
    ctx.set_pixels_per_point(ppp);
    let mut tex = Textures::new();
    let size = vec2(560.0, 330.0);
    let mut sel = Some(1usize);
    let mut on = true;
    let mut check = true;
    let mut v = 0.6_f32;
    let mut out = None;
    for _ in 0..3 {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), ..Default::default() };
        let mut frame_out = ctx.run_ui(input, |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, PANEL);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                tray(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for k in crate::model::Kind::MAIN {
                            icon_button(ui, 38.0, "", k == crate::model::Kind::Torus, |p, r, ink| {
                                crate::glyphs::kind(p, r, k, ink, Color32::from_white_alpha(18))
                            });
                        }
                    });
                });
                tray(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (i, (o, c)) in [(crate::model::Op::Add, ADD), (crate::model::Op::Subtract, CUT), (crate::model::Op::Paint, PAINT)]
                            .into_iter()
                            .enumerate()
                        {
                            let on = i == 0;
                            icon_button(ui, 38.0, "", on, |p, r, _| crate::glyphs::op(p, r, o, if on { Color32::WHITE } else { c }));
                        }
                    });
                });
                tray(ui, |ui| {
                    icon_button(ui, 38.0, "", false, crate::glyphs::group);
                });
                icon_button(ui, 36.0, "", false, crate::glyphs::undo);
                icon_button(ui, 36.0, "", false, crate::glyphs::redo);
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(280.0);
                    segmented(ui, "t", &[("形", ""), ("ざいしつ", ""), ("メルト", ""), ("ライト・背景", "")], sel, true);
                    title(ui, "かたち");
                    xyz(ui, "位置", &mut [1.0, 2.0, 3.0], 0.5, -10.0..=10.0, "");
                    labeled_slider(ui, "とけあい", &mut v, 0.0..=1.0, "");
                    labeled_slider(ui, "金属", &mut 0.0, 0.0..=1.0, "");
                    ui.horizontal(|ui| {
                        checkbox(ui, &mut check, "グループ全部に");
                        switch(ui, &mut on, "グリッド");
                    });
                    ui.horizontal(|ui| {
                        pill(ui, "足す", true);
                        pill(ui, "削る", false);
                        pill_enabled(ui, "ぬる", false, false);
                    });
                });
                card().show(ui, |ui| {
                    ui.set_width(200.0);
                    ui.label("scene");
                    ui.horizontal(|ui| {
                        for k in crate::model::Kind::MAIN {
                            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                            crate::glyphs::kind(ui.painter(), r, k, TEXT, Color32::TRANSPARENT);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (name, col) in [("a", [0.9, 0.3, 0.3]), ("b", [0.4, 0.7, 1.0]), ("c", [1.0, 0.8, 0.3]), ("d", [0.3, 0.3, 0.35])] {
                            let _ = name;
                            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                            let m = crate::model::Material { color: col, roughness: 0.3, ..Default::default() };
                            crate::materials::swatch(ui.painter(), r.center(), 12.0, &m);
                        }
                    });
                });
            });
        });
        apply_deltas(&mut tex, &frame_out.textures_delta);
        frame_out.textures_delta.clear();
        out = Some(frame_out);
        sel = Some(1);
    }
    let Some(out) = out else { return };
    let prims = ctx.tessellate(out.shapes, ppp);
    let (w, h) = ((size.x * ppp) as usize, (size.y * ppp) as usize);
    let bg = [PANEL.r() as f32 / 255.0, PANEL.g() as f32 / 255.0, PANEL.b() as f32 / 255.0];
    let img = raster(&prims, &tex, w, h, ppp, bg);
    let mut data = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in img {
        data.extend(px.iter().map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    }
    let path = std::env::var("YURA_PREVIEW").unwrap_or_else(|_| "yura_preview.ppm".into());
    assert!(std::fs::write(path, data).is_ok());
}

pub(crate) fn write_ppm(img: Vec<[f32; 3]>, w: usize, h: usize, path: &str) {
    let mut data = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in img {
        data.extend(px.iter().map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    }
    assert!(std::fs::write(path, data).is_ok());
}
