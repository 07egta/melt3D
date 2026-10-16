//! The viewport on the GPU: one full-screen triangle whose shader sphere-traces the blob scene
//! (`shader.wgsl`). The scene travels in one uniform buffer. The picture is rendered into an
//! offscreen texture only when its inputs changed (see `perf::FrameCache`); every egui frame just
//! copies that texture into the window.

use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::camera::{Camera, TAN_HALF_FOV};
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick, Scene};
use crate::sdf::Prepared;

const V4_GLOBALS: usize = 7;
const V4_GROUPS: usize = V4_GLOBALS;
const V4_SHAPES: usize = V4_GROUPS + MAX_GROUPS;
const V4_PER_SHAPE: usize = 13;
const V4_POLY: usize = V4_SHAPES + MAX_SHAPES * V4_PER_SHAPE;
const POLY_V4: usize = 512;
const V4_PATH: usize = V4_POLY + POLY_V4;
const PATH_V4: usize = 512;
const V4_BOUND: usize = V4_PATH + PATH_V4;
const V4_HOVER: usize = V4_BOUND + 1;
const V4_ENV: usize = V4_HOVER + 1;
const ENV_V4: usize = 11;
/// Speed-up data for the shader: a bounding radius per shape (64 floats = 16 vec4), then the
/// selection's bounding sphere (centre, radius).
const V4_BRAD: usize = V4_ENV + ENV_V4;
const BRAD_V4: usize = 16;
const _: () = assert!(BRAD_V4 * 4 >= MAX_SHAPES, "a bounding radius slot for every shape");
const V4_SELSPH: usize = V4_BRAD + BRAD_V4;
const TOTAL_V4: usize = V4_SELSPH + 1;
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
    let mut path_n = 0usize;
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
            let pack_outline = |pts: &[[f32; 2]], poly_pts: &mut usize, u: &mut Vec<f32>| -> (usize, usize) {
                if pts.is_empty() || *poly_pts + pts.len() > POLY_V4 * 2 {
                    return (0, 0);
                }
                let start = *poly_pts;
                for (k, pt) in pts.iter().enumerate() {
                    let idx = start + k;
                    let slot = V4_POLY + idx / 2;
                    let off = (idx % 2) * 2;
                    if let Some(dst) = u.get_mut(slot * 4 + off..slot * 4 + off + 2) {
                        dst.copy_from_slice(pt);
                    }
                }
                *poly_pts += pts.len();
                (start, pts.len())
            };
            // (the prepared copy: a revolve profile is already kept on the +x side)
            let (start, count) = if s.kind.uses_outline() { pack_outline(&p.outline, &mut poly_pts, &mut u) } else { (0, 0) };
            let (start2, count2) = if s.kind.uses_outline2() { pack_outline(&p.outline2, &mut poly_pts, &mut u) } else { (0, 0) };
            let (pstart, pcount) = if s.kind == crate::model::Kind::Curve {
                pack_curve(s, &mut path_n, &mut u)
            } else if s.kind.uses_path() && path_n + s.path.len() <= PATH_V4 {
                let st = path_n;
                for (k, pt) in s.path.iter().enumerate() {
                    put(&mut u, V4_PATH + st + k, *pt);
                }
                path_n += s.path.len();
                (st, s.path.len())
            } else {
                (0, 0)
            };
            put(&mut u, base + 6, [s.op.sign(), s.blend, start as f32, count as f32]);
            put(&mut u, base + 10, s.params);
            put(&mut u, base + 12, [p.shell, 0.0, 0.0, 0.0]);
            put(&mut u, base + 11, [pstart as f32, pcount as f32, start2 as f32, count2 as f32]);
            put(&mut u, base + 7, [s.material.color[0], s.material.color[1], s.material.color[2], s.material.roughness]);
            let m = &s.material;
            put(&mut u, base + 8, [m.metallic, m.glow, m.glass, m.iridescence]);
            put(&mut u, base + 9, [m.coat, m.translucency, 0.0, 0.0]);
            if let Some(dst) = u.get_mut(V4_BRAD * 4 + shape_n) {
                *dst = crate::perf::shape_radius(s);
            }
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
    put(&mut u, V4_SELSPH, crate::perf::sphere_of(scene, &crate::view::targets(scene, selected)));
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

/// A curve's smoothed points go into the path list behind two header entries (the low and high
/// corner of a box around the tube). Returns (first point, count); (0, 0) when there is no room.
fn pack_curve(s: &crate::model::Shape, path_n: &mut usize, u: &mut [f32]) -> (usize, usize) {
    let room = PATH_V4.saturating_sub(*path_n + 2);
    let mut pts = crate::curve::build(&s.path, &s.params);
    if pts.len() > room {
        pts = crate::curve::build_max(&s.path, &s.params, room);
    }
    if pts.len() < 2 || pts.len() > room {
        return (0, 0);
    }
    let (lo, hi) = crate::curve::bounds(&pts);
    put(u, V4_PATH + *path_n, [lo[0], lo[1], lo[2], 0.0]);
    put(u, V4_PATH + *path_n + 1, [hi[0], hi[1], hi[2], 0.0]);
    let st = *path_n + 2;
    for (k, pt) in pts.iter().enumerate() {
        put(u, V4_PATH + st + k, *pt);
    }
    *path_n = st + pts.len();
    (st, pts.len())
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

/// Set the shader's flag word (`G.view.w`): see `perf::view_flags`.
pub fn set_view_flags(data: &mut [f32], flags: f32) {
    if let Some(dst) = data.get_mut(4 * 4 + 3) {
        *dst = flags;
    }
}

struct Resources {
    /// Draws the scene (into `target`).
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Copies `target` into egui's picture.
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    format: wgpu::TextureFormat,
    /// The finished picture of the scene. It stays until the scene changes, so a repaint that
    /// is only about the panels costs one textured triangle, not a ray march.
    target: Option<Target>,
    cache: crate::perf::FrameCache,
}

struct Target {
    view: wgpu::TextureView,
    size: [u32; 2],
    /// Smooth copy (the picture is smaller than the viewport) and exact copy (same size).
    linear: wgpu::BindGroup,
    nearest: wgpu::BindGroup,
}

/// The shader is one program in several files: the core, the extra solids, the CAD features.
pub fn shader_source() -> String {
    [include_str!("shader.wgsl"), include_str!("shader_prims.wgsl"), include_str!("shader_cad.wgsl"), include_str!("shader_curve.wgsl")].join("\n")
}

const BLIT_SOURCE: &str = include_str!("blit.wgsl");

/// Build the pipelines (this is the slow part of start-up: the driver compiles the shader).
fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Resources {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("yuracad"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    crate::perf::mark("shader module created");
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
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    crate::perf::mark("scene pipeline created");
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

    // the copy of the finished picture into egui's frame
    let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("yuracad blit"), source: wgpu::ShaderSource::Wgsl(BLIT_SOURCE.into()) });
    let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("yuracad blit"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("yuracad blit"),
        bind_group_layouts: &[Some(&blit_layout)],
        immediate_size: 0,
    });
    let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("yuracad blit"),
        layout: Some(&blit_pl),
        vertex: wgpu::VertexState { module: &blit_module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &blit_module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = |filter| {
        device.create_sampler(&wgpu::SamplerDescriptor { label: Some("yuracad"), mag_filter: filter, min_filter: filter, ..Default::default() })
    };
    Resources {
        pipeline,
        buffer,
        bind,
        blit,
        blit_layout,
        linear: sampler(wgpu::FilterMode::Linear),
        nearest: sampler(wgpu::FilterMode::Nearest),
        format,
        target: None,
        cache: crate::perf::FrameCache::default(),
    }
}

/// Start building the pipeline once, at start-up. The compile runs on its own thread so that the
/// window can show first; the viewport stays empty until it is done and then a repaint is asked for.
pub fn install(rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
    let device = rs.device.clone();
    let format = rs.target_format;
    let renderer = rs.renderer.clone();
    let ctx = ctx.clone();
    let job = Arc::new(move || {
        let res = build(&device, format);
        renderer.write().callback_resources.insert(res);
        crate::perf::mark("gpu ready");
        ctx.request_repaint();
    });
    let job2 = job.clone();
    if std::thread::Builder::new().name("yuracad-gpu-init".into()).spawn(move || job2()).is_err() {
        job();
    }
}

impl Target {
    fn new(device: &wgpu::Device, res: &Resources, size: [u32; 2]) -> Target {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("yuracad picture"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: res.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let group = |sampler: &wgpu::Sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuracad blit"),
                layout: &res.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        Target { linear: group(&res.linear), nearest: group(&res.nearest), view, size }
    }
}

/// One frame of the viewport, as an egui paint callback.
pub struct Callback {
    pub data: Arc<Vec<f32>>,
    /// The size of the picture to draw, in pixels (the viewport's size times the quality scale).
    pub size: [u32; 2],
}

impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let size = crate::perf::render_size(self.size, 1.0, device.limits().max_texture_dimension_2d);
        if res.target.as_ref().is_none_or(|t| t.size != size) {
            res.target = Some(Target::new(device, res, size));
            res.cache.invalidate();
        }
        // nothing changed since the picture was drawn: keep it
        if !res.cache.needs_render(&self.data, size) {
            return Vec::new();
        }
        let Some(target) = res.target.as_ref() else { return Vec::new() };
        let mut bytes = Vec::with_capacity(UNIFORM_BYTES);
        for v in self.data.iter() {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.resize(UNIFORM_BYTES, 0);
        queue.write_buffer(&res.buffer, 0, &bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("yuracad scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &res.bind, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first scene picture queued");
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let Some(target) = res.target.as_ref() else { return };
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        pass.set_viewport(vp.left_px as f32, vp.top_px as f32, vp.width_px as f32, vp.height_px as f32, 0.0, 1.0);
        pass.set_pipeline(&res.blit);
        let exact = target.size == [vp.width_px as u32, vp.height_px as u32];
        pass.set_bind_group(0, if exact { &target.nearest } else { &target.linear }, &[]);
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

#[cfg(test)]
mod curve_tests {
    use super::*;
    use crate::model::{Group, Kind, Material, Shape};

    fn scene_with_curves(n: usize) -> Scene {
        let mut g = Group::new("g");
        for i in 0..n {
            let mut s = Shape::new(Kind::Curve, [i as f32 * 3.0, 0.0, 10.0], Material::default());
            s.path = (0..32).map(|k| [k as f32 * 2.0, (k * k) as f32 * 0.1, 0.0, 3.0]).collect();
            g.shapes.push(s);
        }
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() }
    }

    #[test]
    fn a_curve_is_packed_as_a_box_then_its_smooth_points() {
        let scene = scene_with_curves(1);
        let u = pack(&scene, &Camera::default(), [800.0, 600.0], None, None, true);
        let ext = (V4_SHAPES + 11) * 4;
        let (start, count) = (u[ext] as usize, u[ext + 1] as usize);
        assert!((2..=crate::curve::MAX_POINTS).contains(&count) && start == 2);
        let at = |slot: usize| &u[(V4_PATH + slot) * 4..(V4_PATH + slot) * 4 + 4];
        let (lo, hi) = (at(start - 2), at(start - 1));
        for k in 0..count {
            let p = at(start + k);
            for i in 0..3 {
                assert!(p[i] >= lo[i] && p[i] <= hi[i], "point {k} axis {i} outside the box");
            }
        }
        let want = crate::curve::build(&scene.groups[0].shapes[0].path, &scene.groups[0].shapes[0].params);
        assert_eq!(want.len(), count);
        assert_eq!(at(start), &want[0][..]);
    }

    #[test]
    fn too_many_curves_never_overflow_the_path_list() {
        let scene = scene_with_curves(40);
        let u = pack(&scene, &Camera::default(), [800.0, 600.0], None, None, true);
        assert_eq!(u.len() * 4, UNIFORM_BYTES);
        let mut used = 0;
        for i in 0..scene.shape_count() {
            let ext = (V4_SHAPES + i * V4_PER_SHAPE + 11) * 4;
            let (start, count) = (u[ext] as usize, u[ext + 1] as usize);
            if count > 0 {
                assert!(start + count <= PATH_V4, "curve {i} runs past the list");
                used = used.max(start + count);
            }
        }
        assert!(used > 0);
    }
}

#[cfg(test)]
mod shader_tests {
    use super::*;

    /// The shader must parse and validate, or the viewport shows nothing.
    #[test]
    fn the_shader_is_valid_wgsl() {
        let src = shader_source();
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        if let Err(e) = v.validate(&module) {
            panic!("{}", e.emit_to_string(&src));
        }
    }

    /// The uniform buffer must be exactly as big as the shader's `Globals` struct, or wgpu rejects
    /// the bind group (and the viewport never draws).
    #[test]
    fn the_shader_struct_is_as_big_as_the_uniform_buffer() {
        let src = shader_source();
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let size = module.types.iter().find_map(|(_, t)| match (&t.name, &t.inner) {
            (Some(name), naga::TypeInner::Struct { span, .. }) if name == "Globals" => Some(*span as usize),
            _ => None,
        });
        assert_eq!(size, Some(UNIFORM_BYTES));
    }

    #[test]
    fn the_blit_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(BLIT_SOURCE).unwrap_or_else(|e| panic!("{}", e.emit_to_string(BLIT_SOURCE)));
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        if let Err(e) = v.validate(&module) {
            panic!("{}", e.emit_to_string(BLIT_SOURCE));
        }
    }

    #[test]
    fn the_uniform_fits_in_the_usual_limit() {
        assert!(UNIFORM_BYTES <= 65536, "{UNIFORM_BYTES}");
    }
}
