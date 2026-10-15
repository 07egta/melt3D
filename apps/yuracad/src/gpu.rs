//! The viewport on the GPU: one full-screen triangle whose shader sphere-traces the blob scene
//! (`shader.wgsl`). The scene travels in one uniform buffer, repacked every frame (a few KB).

use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::camera::{Camera, TAN_HALF_FOV};
use crate::model::{Kind, MAX_GROUPS, MAX_SHAPES, Pick, Scene};
use crate::sdf::Prepared;

const V4_GLOBALS: usize = 7;
const V4_GROUPS: usize = V4_GLOBALS;
const V4_SHAPES: usize = V4_GROUPS + MAX_GROUPS;
const V4_PER_SHAPE: usize = 10;
const V4_POLY: usize = V4_SHAPES + MAX_SHAPES * V4_PER_SHAPE;
const POLY_V4: usize = 128;
const V4_BOUND: usize = V4_POLY + POLY_V4;
const V4_HOVER: usize = V4_BOUND + 1;
const V4_ENV: usize = V4_HOVER + 1;
const ENV_V4: usize = 11;
const TOTAL_V4: usize = V4_ENV + ENV_V4;
/// Size of the uniform buffer in bytes.
pub const UNIFORM_BYTES: usize = TOTAL_V4 * 16;

/// Everything the shader needs for one frame.
fn put(u: &mut [f32], slot: usize, v: [f32; 4]) {
    if let Some(dst) = u.get_mut(slot * 4..slot * 4 + 4) {
        dst.copy_from_slice(&v);
    }
}

pub fn pack(scene: &Scene, cam: &Camera, size: [f32; 2], selected: Option<Pick>, hover: Option<Pick>, srgb: bool) -> Vec<f32> {
    let mut u = vec![0.0_f32; TOTAL_V4 * 4];
    let (right, up, fwd) = cam.basis();
    let eye = cam.eye();
    put(&mut u, 0, [eye[0], eye[1], eye[2], 0.0]);
    put(&mut u, 1, [right[0], right[1], right[2], 0.0]);
    put(&mut u, 2, [up[0], up[1], up[2], 0.0]);
    put(&mut u, 3, [fwd[0], fwd[1], fwd[2], if cam.ortho { 0.0 } else { TAN_HALF_FOV }]);
    put(&mut u, 4, [size[0], size[1], cam.half_height(), if srgb { 1.0 } else { 0.0 }]);
    let m = scene.melt;
    put(&mut u, 5, [m.mode.id(), m.amount, m.drip, m.ripple]);

    let mut shape_n = 0usize;
    let mut group_n = 0usize;
    let mut poly_pts = 0usize;
    let mut sel_first = -1.0_f32;
    let mut sel_count = 0.0_f32;
    let mut hover_first = -1.0_f32;
    let mut hover_count = 0.0_f32;
    for (gi, g) in scene.groups.iter().enumerate() {
        if !g.visible {
            continue;
        }
        let first = shape_n;
        for (si, s) in g.shapes.iter().enumerate() {
            if !s.visible || shape_n >= MAX_SHAPES {
                continue;
            }
            let p = Prepared::new(s);
            let base = V4_SHAPES + shape_n * V4_PER_SHAPE;
            put(&mut u, base, [p.inv[0][0], p.inv[0][1], p.inv[0][2], 0.0]);
            put(&mut u, base + 1, [p.inv[1][0], p.inv[1][1], p.inv[1][2], 0.0]);
            put(&mut u, base + 2, [p.inv[2][0], p.inv[2][1], p.inv[2][2], 0.0]);
            put(&mut u, base + 3, [s.pos[0], s.pos[1], s.pos[2], p.min_scale()]);
            put(&mut u, base + 4, [s.scale[0], s.scale[1], s.scale[2], s.kind.id()]);
            put(&mut u, base + 5, [s.size[0], s.size[1], s.size[2], s.round]);
            let (start, count) = if s.kind == Kind::Extrude && poly_pts + s.outline.len() <= POLY_V4 * 2 {
                let start = poly_pts;
                for (k, pt) in s.outline.iter().enumerate() {
                    let idx = start + k;
                    let slot = V4_POLY + idx / 2;
                    let off = (idx % 2) * 2;
                    if let Some(dst) = u.get_mut(slot * 4 + off..slot * 4 + off + 2) {
                        dst.copy_from_slice(pt);
                    }
                }
                poly_pts += s.outline.len();
                (start, s.outline.len())
            } else {
                (0, 0)
            };
            put(&mut u, base + 6, [s.op.sign(), s.blend, start as f32, count as f32]);
            put(&mut u, base + 7, [s.material.color[0], s.material.color[1], s.material.color[2], s.material.roughness]);
            let m = &s.material;
            put(&mut u, base + 8, [m.metallic, m.glow, m.glass, m.iridescence]);
            put(&mut u, base + 9, [m.coat, m.translucency, 0.0, 0.0]);
            if hover == Some(Pick::Shape(gi, si)) {
                hover_first = shape_n as f32;
                hover_count = 1.0;
            }
            if selected == Some(Pick::Shape(gi, si)) {
                sel_first = shape_n as f32;
                sel_count = 1.0;
            }
            shape_n += 1;
        }
        let count = shape_n - first;
        if selected == Some(Pick::Group(gi)) && count > 0 {
            sel_first = first as f32;
            sel_count = count as f32;
        }
        if count > 0 && group_n < MAX_GROUPS {
            put(&mut u, V4_GROUPS + group_n, [first as f32, count as f32, g.op.sign(), g.blend]);
            group_n += 1;
        }
    }
    put(&mut u, 6, [group_n as f32, shape_n as f32, sel_first, sel_count]);
    let (bc, br) = bounds(scene, selected);
    put(&mut u, V4_BOUND, [bc[0], bc[1], bc[2], br]);
    put(&mut u, V4_HOVER, [hover_first, hover_count, 0.0, 0.0]);
    let e = &scene.env;
    let flags = (if e.grid { 1.0 } else { 0.0 }) + (if e.floor { 2.0 } else { 0.0 });
    put(&mut u, V4_ENV, [e.exposure, e.ambient, e.shadow, flags]);
    put(&mut u, V4_ENV + 1, [e.backdrop[0], e.backdrop[1], e.backdrop[2], e.lights.len() as f32]);
    put(&mut u, V4_ENV + 2, [e.sky_top[0], e.sky_top[1], e.sky_top[2], 0.0]);
    put(&mut u, V4_ENV + 3, [e.sky_horizon[0], e.sky_horizon[1], e.sky_horizon[2], 0.0]);
    put(&mut u, V4_ENV + 4, [e.sky_bottom[0], e.sky_bottom[1], e.sky_bottom[2], 0.0]);
    for (i, l) in e.lights.iter().enumerate().take(crate::model::MAX_LIGHTS) {
        let d = l.dir();
        put(&mut u, V4_ENV + 5 + i * 2, [d[0], d[1], d[2], l.power]);
        put(&mut u, V4_ENV + 6 + i * 2, [l.color[0], l.color[1], l.color[2], 0.0]);
    }
    u
}

/// A sphere around everything the shader can draw (centre, radius; radius 0 when there is nothing):
/// rays that miss it skip the marching. It includes the melt's reach, the drips and the selection.
pub fn bounds(scene: &Scene, selected: Option<Pick>) -> ([f32; 3], f32) {
    let mut items: Vec<([f32; 3], f32)> = Vec::new();
    for (gi, g) in scene.groups.iter().enumerate() {
        if !g.visible {
            continue;
        }
        for (si, s) in g.shapes.iter().enumerate() {
            let is_sel = match selected {
                Some(Pick::Shape(a, b)) => a == gi && b == si,
                Some(Pick::Group(a)) => a == gi,
                None => false,
            };
            if !s.visible || (!is_sel && (s.op != crate::model::Op::Add || g.op != crate::model::Op::Add)) {
                continue;
            }
            // a lamp's glow reaches a good way out
            let halo = if s.material.glow > 0.0 { 30.0 } else { 0.0 };
            let reach = (s.blend + g.blend) * scene.melt.amount * 1.5 + scene.melt.drip + 2.0 + halo;
            items.push((s.pos, crate::view::radius_of(s) + reach));
        }
    }
    if items.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let n = items.len() as f32;
    let c = items.iter().fold([0.0; 3], |a, (p, _)| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
    let r = items.iter().map(|(p, r)| crate::sdf::len(crate::sdf::sub(*p, c)) + r).fold(0.0_f32, f32::max);
    (c, r)
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

/// Build the pipeline once; call at start-up with eframe's render state.
pub fn install(rs: &egui_wgpu::RenderState) {
    let device = &rs.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("yuracad"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("yuracad"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("yuracad"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("yuracad"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState { format: rs.target_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("yuracad"),
        contents: &vec![0u8; UNIFORM_BYTES],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("yuracad"),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
    });
    rs.renderer.write().callback_resources.insert(Resources { pipeline, buffer, bind });
}

/// One frame of the viewport, as an egui paint callback.
pub struct Callback {
    pub data: Arc<Vec<f32>>,
}

impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(res) = resources.get::<Resources>() {
            let mut bytes = Vec::with_capacity(UNIFORM_BYTES);
            for v in self.data.iter() {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.resize(UNIFORM_BYTES, 0);
            queue.write_buffer(&res.buffer, 0, &bytes);
        }
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        pass.set_viewport(vp.left_px as f32, vp.top_px as f32, vp.width_px as f32, vp.height_px as f32, 0.0, 1.0);
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &res.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_scene_packs_into_the_uniform_without_overflow() {
        let u = pack(&Scene::default(), &Camera::default(), [800.0, 600.0], Some(Pick::Shape(0, 0)), None, true);
        assert_eq!(u.len() * 4, UNIFORM_BYTES);
        assert_eq!(&u[24..28], &[1.0, 1.0, 0.0, 1.0], "one group, one shape, shape 0 selected");
        let e = V4_ENV * 4;
        assert!((u[e] - 1.0).abs() < 1e-6 && (u[e + 7] - 2.0).abs() < 1e-6, "exposure 1, two lights");
    }
}
